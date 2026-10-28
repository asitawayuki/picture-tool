//! メモリ予算（spec 2026-10-28）
//!
//! **画像を開く処理は、開く前に「この 1 枚のピーク見積もり」をプロセス共通の予算から借りる。**
//! 借りられるまで待ち、1 枚の処理が終わったとき（成功・失敗・panic のどれでも）に [`Lease`] の
//! drop で返す。見積もりは [`estimate`] にある。
//!
//! - **順番は到着順。** 大きい 1 枚が待っている間、後から来た小さい 1 枚は追い越さない
//!   （追い越しを許すと大きい 1 枚が飢餓になる）。`Condvar::notify_all` の起床順は保証されないので、
//!   到着順の券（`next_ticket`）と「次に借りてよい券」（`serving`）で順番を明示的に守る
//! - **予算を超え上限以下の 1 枚は単独で処理する。** 自分の番が来て、かつ誰も借りていないときに借りる。
//!   借りている間は `in_use > budget` なので、後ろは自然に待つ
//! - **上限を超える 1 枚は待たずに [`OverMemoryLimit`] で失敗する。** 待っても借りられる日は来ない
//! - **入れ子で借りない。** 1 枚の処理の中で 2 度借りると、到着順の下でデッドロックしうる。
//!   同じスレッドが借りたまま再び借りようとしたら `Err` にする（予算が別でも同じ）

use anyhow::{Result, bail};
use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

const GB: f64 = 1e9;

/// 総量が取れないときにみなす物理メモリの総量。「8GB の PC」（OS が報告する総量 7.5GiB。spec §1）と同じ
const FALLBACK_TOTAL: u64 = 7 * (1 << 30) + (1 << 29);

/// 物理メモリの総量から `(予算, 上限)` を決める。予算は総量の 1/2、上限は 3/4。
/// 総量が取れない（`None`）ときは [`FALLBACK_TOTAL`] とみなす
pub(crate) fn from_total(total: Option<u64>) -> (u64, u64) {
    let total = total.unwrap_or(FALLBACK_TOTAL);
    (total / 2, total / 4 * 3)
}

/// プロセス全体で 1 つの予算。初回に 1 回だけ物理メモリの総量を読む
/// （「空き」ではなく「総量」を使うのは、結果を実行のたびに変えないため。spec §1）。
/// テストはこれを差し替えず、自分で作った [`MemoryBudget`] を渡す
pub(crate) fn global() -> &'static MemoryBudget {
    static GLOBAL: OnceLock<MemoryBudget> = OnceLock::new();
    GLOBAL.get_or_init(|| {
        use sysinfo::{MemoryRefreshKind, RefreshKind, System};
        let sys = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        let total = Some(sys.total_memory()).filter(|&t| t > 0);
        let (budget, limit) = from_total(total);
        MemoryBudget::new(budget, limit)
    })
}

/// 3/4 の上限を超える見積もりで弾いたときの `Err`。呼び出し側は `downcast_ref` で見分けられる
#[derive(Debug)]
pub struct OverMemoryLimit {
    pub needed: u64,
    pub limit: u64,
}

impl fmt::Display for OverMemoryLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Image needs about {:.1} GB, over the memory limit of {:.1} GB",
            self.needed as f64 / GB,
            self.limit as f64 / GB
        )
    }
}

impl std::error::Error for OverMemoryLimit {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Wait,
    Acquire,
    Release,
}

/// 借用の記録（計測とテスト用。[`MemoryBudget::with_log`] で作った予算だけが取る）。
/// `in_use`・`holders` はその事象の**後**の値、`at` は予算を作ってからの経過
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub at: Duration,
    pub kind: EventKind,
    pub estimate: u64,
    pub in_use: u64,
    pub holders: usize,
}

struct State {
    in_use: u64,
    holders: usize,
    next_ticket: u64,
    serving: u64,
    log: Option<Vec<Event>>,
}

pub struct MemoryBudget {
    budget: u64,
    limit: u64,
    created: Instant,
    state: Mutex<State>,
    freed: Condvar,
}

thread_local! {
    /// このスレッドがいま借りているか（入れ子の検知。予算をまたいでスレッドごとに 1 つ）
    static HOLDING: Cell<bool> = const { Cell::new(false) };
}

impl MemoryBudget {
    /// `limit < budget` は `budget` に持ち上げる（予算以下の 1 枚は必ず借りられる）
    pub fn new(budget: u64, limit: u64) -> Self {
        Self::build(budget, limit, None)
    }

    /// 借用の記録（[`MemoryBudget::events`]）を取る予算。プロセス全体の予算は記録を取らない
    /// （GUI はサムネイルのたびに借りるので、記録が際限なく伸びる）
    pub fn with_log(budget: u64, limit: u64) -> Self {
        Self::build(budget, limit, Some(Vec::new()))
    }

    fn build(budget: u64, limit: u64, log: Option<Vec<Event>>) -> Self {
        Self {
            budget,
            limit: limit.max(budget),
            created: Instant::now(),
            state: Mutex::new(State {
                in_use: 0,
                holders: 0,
                next_ticket: 0,
                serving: 0,
                log,
            }),
            freed: Condvar::new(),
        }
    }

    pub fn budget(&self) -> u64 {
        self.budget
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }

    pub fn in_use(&self) -> u64 {
        self.lock().in_use
    }

    pub fn events(&self) -> Vec<Event> {
        self.lock().log.clone().unwrap_or_default()
    }

    /// `estimate` バイトを借りる。借りられるまで待つ
    pub fn acquire(&self, estimate: u64) -> Result<Lease<'_>> {
        if estimate > self.limit {
            return Err(OverMemoryLimit {
                needed: estimate,
                limit: self.limit,
            }
            .into());
        }
        if HOLDING.get() {
            bail!("memory budget lease is already held by this thread");
        }
        let mut st = self.lock();
        let ticket = st.next_ticket;
        st.next_ticket += 1;
        if !self.can_take(&st, ticket, estimate) {
            self.record(&mut st, EventKind::Wait, estimate);
            while !self.can_take(&st, ticket, estimate) {
                st = self
                    .freed
                    .wait(st)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        }
        st.serving += 1;
        st.in_use += estimate;
        st.holders += 1;
        self.record(&mut st, EventKind::Acquire, estimate);
        drop(st);
        // 券が進んだので、次の券の持ち主が借りられるかもしれない
        self.freed.notify_all();
        HOLDING.set(true);
        Ok(Lease {
            budget: self,
            estimate,
            _not_send: PhantomData,
        })
    }

    fn can_take(&self, st: &State, ticket: u64, estimate: u64) -> bool {
        ticket == st.serving
            && (st.in_use.saturating_add(estimate) <= self.budget
                || (estimate > self.budget && st.holders == 0))
    }

    fn record(&self, st: &mut State, kind: EventKind, estimate: u64) {
        let (in_use, holders) = (st.in_use, st.holders);
        if let Some(log) = st.log.as_mut() {
            log.push(Event {
                at: self.created.elapsed(),
                kind,
                estimate,
                in_use,
                holders,
            });
        }
    }

    /// panic したワーカーの後も他が借りられるよう、poison は無視して続ける
    /// （状態の更新は panic しうる処理を挟まずに終わるので、壊れた状態は残らない）
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// 借りた分。drop で返す。スレッドの印と結びつくので別スレッドへ渡せない（`!Send`）
pub struct Lease<'a> {
    budget: &'a MemoryBudget,
    estimate: u64,
    _not_send: PhantomData<*const ()>,
}

impl fmt::Debug for Lease<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lease")
            .field("estimate", &self.estimate)
            .finish()
    }
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        let b = self.budget;
        let mut st = b.lock();
        st.in_use -= self.estimate;
        st.holders -= 1;
        b.record(&mut st, EventKind::Release, self.estimate);
        drop(st);
        b.freed.notify_all();
        HOLDING.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_until(cond: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !cond() {
            assert!(
                Instant::now() < deadline,
                "条件が 10 秒以内に成り立たなかった"
            );
            std::thread::yield_now();
        }
    }

    fn waits(b: &MemoryBudget) -> usize {
        b.events()
            .iter()
            .filter(|e| e.kind == EventKind::Wait)
            .count()
    }

    fn acquires(b: &MemoryBudget) -> Vec<u64> {
        b.events()
            .iter()
            .filter(|e| e.kind == EventKind::Acquire)
            .map(|e| e.estimate)
            .collect()
    }

    fn position(ev: &[Event], kind: EventKind, estimate: u64) -> usize {
        ev.iter()
            .position(|e| e.kind == kind && e.estimate == estimate)
            .unwrap_or_else(|| panic!("{kind:?} {estimate} が記録に無い"))
    }

    #[test]
    fn budget_is_half_and_limit_is_three_quarters_of_total() {
        assert_eq!(
            from_total(Some(62_000_000_000)),
            (31_000_000_000, 46_500_000_000)
        );
    }

    #[test]
    fn unknown_total_is_treated_as_a_7_5_gib_machine() {
        assert_eq!(from_total(None), (4_026_531_840, 6_039_797_760));
        assert_eq!(
            from_total(None),
            from_total(Some(7 * (1 << 30) + (1 << 29)))
        );
    }

    #[test]
    fn later_small_request_does_not_overtake_a_waiting_large_one() {
        let b = MemoryBudget::with_log(100, 150);
        let a = b.acquire(60).unwrap();
        // B(50) は入らない（60+50>100）。C(30) は追い越しを許せば入る
        assert!(
            60 + 30 <= b.budget(),
            "前提: C は追い越しを許せば入る大きさ"
        );
        std::thread::scope(|s| {
            s.spawn(|| drop(b.acquire(50).unwrap())); // B
            wait_until(|| waits(&b) == 1);
            s.spawn(|| drop(b.acquire(30).unwrap())); // C
            wait_until(|| waits(&b) == 2);
            assert_eq!(acquires(&b), vec![60], "B が待つ間、C は借りていない");
            drop(a);
        });
        assert_eq!(acquires(&b), vec![60, 50, 30], "到着順");
    }

    #[test]
    fn over_budget_request_runs_alone() {
        let b = MemoryBudget::with_log(100, 150);
        let a = b.acquire(30).unwrap();
        std::thread::scope(|s| {
            s.spawn(|| drop(b.acquire(120).unwrap())); // 予算超え・上限以下
            wait_until(|| waits(&b) == 1);
            s.spawn(|| drop(b.acquire(10).unwrap()));
            wait_until(|| waits(&b) == 2);
            drop(a);
        });
        // 120 は holders == 1 で借り、10 は 120 の Release より後に借りる
        let ev = b.events();
        let solo = position(&ev, EventKind::Acquire, 120);
        assert_eq!(ev[solo].holders, 1);
        assert!(position(&ev, EventKind::Acquire, 10) > position(&ev, EventKind::Release, 120));
    }

    #[test]
    fn over_limit_request_fails_without_waiting() {
        let b = MemoryBudget::with_log(100, 150);
        let _held = b.acquire(100).unwrap(); // 待つなら永遠に待つ状態
        let err = b.acquire(151).unwrap_err();
        let over = err.downcast_ref::<OverMemoryLimit>().unwrap();
        assert_eq!((over.needed, over.limit), (151, 150));
        assert_eq!(waits(&b), 0);
    }

    #[test]
    fn over_limit_message_names_both_sizes_in_gb() {
        let e = OverMemoryLimit {
            needed: 7_100_000_000,
            limit: 6_000_000_000,
        };
        assert_eq!(
            e.to_string(),
            "Image needs about 7.1 GB, over the memory limit of 6.0 GB"
        );
    }

    #[test]
    fn lease_is_returned_on_panic() {
        let b = MemoryBudget::new(100, 150);
        let r = std::thread::scope(|s| {
            s.spawn(|| {
                let _l = b.acquire(100).unwrap();
                panic!("x")
            })
            .join()
        });
        assert!(r.is_err());
        assert_eq!(b.in_use(), 0);
        // poison されても、後から借りられる
        assert!(b.acquire(100).is_ok());
    }

    #[test]
    fn second_lease_on_the_same_thread_is_an_error() {
        let b = MemoryBudget::new(100, 150);
        let _l = b.acquire(10).unwrap();
        let err = b.acquire(10).unwrap_err();
        assert!(format!("{err:#}").contains("memory budget lease is already held by this thread"));
        // 別の予算でも同じ（入れ子の禁止は予算ごとではなくスレッドごと）
        assert!(MemoryBudget::new(100, 150).acquire(10).is_err());
    }

    #[test]
    fn the_thread_can_borrow_again_after_returning() {
        let b = MemoryBudget::new(100, 150);
        drop(b.acquire(10).unwrap());
        assert!(b.acquire(10).is_ok());
    }
}
