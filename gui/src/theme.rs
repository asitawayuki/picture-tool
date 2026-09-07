//! OS のテーマ（明暗）の取得と、選ばれたテーマのネイティブ側への反映。
//!
//! **Linux では OS 追従を自前で持つしかない。** 実測して確かめた前提:
//!
//!   - WebKitGTK の `prefers-color-scheme` は GTK の
//!     `gtk-application-prefer-dark-theme` に**ライブで**追従する
//!   - GTK3 (3.24) は GNOME の `color-scheme` 設定を読まない
//!     （libgtk-3 に xdg-desktop-portal の appearance 参照が無い）
//!   - tao の Linux 実装はテーマ名の `-dark` サフィックス判定だけで、
//!     `WindowEvent::ThemeChanged` は Linux では**発火しない**
//!
//! そのため何もしないと、GNOME をダークにしてもアプリは永久にライトのままになる。
//! ここで xdg-desktop-portal の `org.freedesktop.appearance / color-scheme` を
//! 直接読み、変化を webview へ流す。D-Bus の口は tao 経由で既にビルド済みの
//! `gio` を使う（新しい依存を積まないため）。
//!
//! Linux 以外は OS 側の仕組みが正しく動くので、tao の値と `ThemeChanged` に任せる。

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, Theme};

/// OS の明暗が変わったときに webview へ流すイベント名。
/// ペイロードは `"light"` / `"dark"` の文字列。
pub const OS_SCHEME_CHANGED_EVENT: &str = "os-color-scheme-changed";

pub const LIGHT: &str = "light";
pub const DARK: &str = "dark";

/// 現在の OS の明暗。起動時に確定させ、変化のたびに更新する。
///
/// webview から毎回 D-Bus を叩かせないための置き場でもある
/// （問い合わせは起動時の 1 回だけで済む）。
pub struct OsColorScheme(AtomicBool);

impl OsColorScheme {
    pub fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// `Relaxed` で足りる。このフラグは他のメモリ操作と順序関係を持たず、
    /// 「最後に書かれた値が読めればよい」だけである（`main.rs` の
    /// `close_prompt_open` のように 2 つのフラグの順序が意味を持つ場所とは違う）。
    fn store(&self, dark: bool) {
        self.0.store(dark, Ordering::Relaxed);
    }

    /// 書き込んで、**変化があったか**を返す
    fn replace(&self, dark: bool) -> bool {
        self.0.swap(dark, Ordering::Relaxed) != dark
    }

    fn as_str(&self) -> &'static str {
        if self.0.load(Ordering::Relaxed) {
            DARK
        } else {
            LIGHT
        }
    }
}

impl Default for OsColorScheme {
    fn default() -> Self {
        Self::new()
    }
}

/// 起動時の OS の明暗を返す。
///
/// 追従の本体はイベント（[`OS_SCHEME_CHANGED_EVENT`]）側で、こちらは初期値専用。
#[tauri::command]
pub fn os_color_scheme(state: tauri::State<'_, OsColorScheme>) -> &'static str {
    state.as_str()
}

/// 解決後のテーマをウィンドウへ伝える。
///
/// これでネイティブのダイアログ（削除確認・書き込み承認）とウィンドウ枠が
/// アプリ内の明暗と揃う。**webview から来る値なので 2 値以外は受け付けない**
/// （`security.rs` と同じ線。ここは自作コマンドで ACL が効かない）。
///
/// **反映するのは Linux だけ。** tao の `set_theme` は macOS / Windows で
/// `WindowEvent::ThemeChanged` を誘発しうる。それが OS 由来なのかアプリ自身の
/// 上書きなのかを区別する手立てが無く、区別せずに受け取ると「利用者がダークを
/// 選んだ」を「OS がダークになった」と記録してしまう ── 後でシステム追従へ
/// 戻したときに、実際の OS 設定ではない古い値が出る。
/// **窓枠を揃えることより追従の正しさを採る。** Linux では `ThemeChanged` 自体が
/// 発火しないので、この取り違えは起きない。
#[tauri::command]
pub fn set_window_theme(window: tauri::Window, theme: String) -> Result<(), String> {
    let theme = match theme.as_str() {
        LIGHT => Theme::Light,
        DARK => Theme::Dark,
        other => return Err(format!("未知のテーマです: {other}")),
    };

    #[cfg(target_os = "linux")]
    return window.set_theme(Some(theme)).map_err(|e| e.to_string());

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, theme);
        Ok(())
    }
}

/// OS の明暗の初期値を確定させ、変化の購読を始める。
///
/// `setup` から 1 回だけ呼ぶ。**イベントループが回り出す前に終わる**ので、
/// webview が `os_color_scheme` を呼ぶ時点では実値が入っている
/// （初期値の `light` が webview へ漏れることはない）。
/// 購読に失敗しても（portal が無い等）起動は続ける ── そのときは
/// 「追従しないライト固定」に劣化するだけで、手動の切替は生きている。
pub fn init(app: &AppHandle) {
    let initial = platform::watch(app.clone());
    app.state::<OsColorScheme>().store(initial);
}

/// Linux 以外で tao がテーマ変更を報せてきたときの受け口。
///
/// Linux では発火しないので、この関数も呼ばれない（`main.rs` 側で cfg 済み）。
#[cfg(not(target_os = "linux"))]
pub fn on_theme_changed(app: &AppHandle, theme: Theme) {
    publish(app, matches!(theme, Theme::Dark));
}

/// 明暗の変化を状態へ書き、webview へ流す。
///
/// **値が実際に変わったときだけ流す。** portal は 1 回の切り替えで
/// `SettingChanged` を複数回出すことがある（GNOME で 2 回来るのを実測）。
fn publish(app: &AppHandle, dark: bool) {
    let state = app.state::<OsColorScheme>();
    if !state.replace(dark) {
        return;
    }
    let _ = app.emit(OS_SCHEME_CHANGED_EVENT, state.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// portal は 1 回の切り替えで `SettingChanged` を複数回出す（GNOME で 2 回来るのを実測）。
    /// **同じ値の再通知で webview を起こさない**のがこの型の役目である。
    #[test]
    fn reports_only_actual_changes() {
        let scheme = OsColorScheme::new();
        assert_eq!(scheme.as_str(), LIGHT);

        assert!(scheme.replace(true), "light から dark は変化");
        assert_eq!(scheme.as_str(), DARK);
        assert!(!scheme.replace(true), "同じ値の再通知は変化ではない");
        assert!(scheme.replace(false), "dark から light は変化");
        assert_eq!(scheme.as_str(), LIGHT);
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::publish;
    use gio::glib::Variant;
    use gio::prelude::*;
    use tauri::AppHandle;

    const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
    const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
    const PORTAL_SETTINGS: &str = "org.freedesktop.portal.Settings";
    const APPEARANCE_NS: &str = "org.freedesktop.appearance";
    const COLOR_SCHEME_KEY: &str = "color-scheme";

    /// 応答待ちの上限。起動を止める場所なので短く切る（セッションバス上の
    /// ローカル呼び出しで、通常はミリ秒で返る）。
    const CALL_TIMEOUT_MS: i32 = 1_000;

    /// `org.freedesktop.appearance / color-scheme` の値。
    /// 0 = 指定なし、1 = ダーク志向、2 = ライト志向。
    fn is_dark(code: u32) -> bool {
        code == 1
    }

    /// portal の返り値から明暗を取り出す。
    ///
    /// 包み方が版によって違う（`ReadOne` は `(v)`、`Read` は `v` をもう一段包む）。
    /// 形を決め打ちにせず、タプルと variant をほどいて最初に見つかった u32 を採る。
    ///
    /// 再帰の深さは GVariant 自体の入れ子上限（GLib が解析時に弾く）で頭打ちになる
    /// ので、深さの番人は置いていない。
    fn dark_from_variant(value: &Variant) -> Option<bool> {
        if let Some(code) = value.get::<u32>() {
            return Some(is_dark(code));
        }
        // **型を確かめてから開く。** `as_variant()` は `v` 以外を渡すと
        // GLib の critical を出す（実測。`G_DEBUG=fatal-criticals` では落ちる）
        if value.is::<Variant>() {
            if let Some(inner) = value.as_variant() {
                return dark_from_variant(&inner);
            }
        }
        if value.type_().is_tuple() {
            return (0..value.n_children()).find_map(|i| dark_from_variant(&value.child_value(i)));
        }
        None
    }

    fn proxy() -> Option<gio::DBusProxy> {
        gio::DBusProxy::for_bus_sync(
            gio::BusType::Session,
            gio::DBusProxyFlags::NONE,
            None,
            PORTAL_BUS,
            PORTAL_PATH,
            PORTAL_SETTINGS,
            gio::Cancellable::NONE,
        )
        .ok()
    }

    fn read_current(proxy: &gio::DBusProxy) -> Option<bool> {
        let args = (APPEARANCE_NS, COLOR_SCHEME_KEY).to_variant();
        // `ReadOne` は Settings v2。古い portal しか無い環境では `Read` へ落ちる
        ["ReadOne", "Read"].into_iter().find_map(|method| {
            proxy
                .call_sync(
                    method,
                    Some(&args),
                    gio::DBusCallFlags::NONE,
                    CALL_TIMEOUT_MS,
                    gio::Cancellable::NONE,
                )
                .ok()
                .and_then(|reply| dark_from_variant(&reply))
        })
    }

    /// portal を購読し、現在値を返す。
    ///
    /// proxy は意図的に leak する。購読を生かすには生存させ続ける必要があり、
    /// かつアプリの寿命と一致するので、状態として持ち回る意味が無い
    /// （`gio::DBusProxy` は `Send` でないので Tauri の管理状態には置けない）。
    pub fn watch(app: AppHandle) -> bool {
        let Some(proxy) = proxy() else {
            return false; // portal が無い環境。追従しないだけで起動は続ける
        };
        let current = read_current(&proxy).unwrap_or(false);

        let proxy: &'static gio::DBusProxy = Box::leak(Box::new(proxy));
        proxy.connect_g_signal(move |_, _sender, signal, params| {
            if signal != "SettingChanged" || params.n_children() < 3 {
                return;
            }
            let namespace = params.child_value(0).get::<String>();
            let key = params.child_value(1).get::<String>();
            if namespace.as_deref() != Some(APPEARANCE_NS)
                || key.as_deref() != Some(COLOR_SCHEME_KEY)
            {
                return;
            }
            if let Some(dark) = dark_from_variant(&params.child_value(2)) {
                publish(&app, dark);
            }
        });

        current
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// 値の意味は xdg-desktop-portal の appearance 仕様
        /// （0 = 指定なし、1 = ダーク志向、2 = ライト志向）。
        /// 「指定なし」をダークに倒さないことが要点である。
        #[test]
        fn color_scheme_codes() {
            assert!(!is_dark(0));
            assert!(is_dark(1));
            assert!(!is_dark(2));
            assert!(!is_dark(3), "仕様外の値はライト側へ倒す");
        }

        #[test]
        fn unwraps_read_one_reply() {
            // ReadOne の返り値: (v) の中に u32
            let reply = (1u32.to_variant(),).to_variant();
            assert_eq!(dark_from_variant(&reply), Some(true));
        }

        #[test]
        fn unwraps_doubly_wrapped_read_reply() {
            // Read の返り値: variant がもう一段包まれている
            let reply = (1u32.to_variant().to_variant(),).to_variant();
            assert_eq!(dark_from_variant(&reply), Some(true));
        }

        #[test]
        fn unwraps_signal_payload() {
            // SettingChanged の第 3 引数は生の variant
            assert_eq!(dark_from_variant(&2u32.to_variant()), Some(false));
        }

        #[test]
        fn walks_past_a_string_without_opening_it() {
            // 文字列を variant として開こうとすると GLib の critical が出る。
            // タプルの途中に数値でない値が混じっても、そこで止まらず先を見る
            let reply = ("prefer-dark", 1u32).to_variant();
            assert_eq!(dark_from_variant(&reply), Some(true));
        }

        #[test]
        fn rejects_payload_without_a_number() {
            // 形が変わったときに「ダークだと思い込む」より、読めないと言う方が安全
            let reply = ("prefer-dark".to_variant(),).to_variant();
            assert_eq!(dark_from_variant(&reply), None);
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use tauri::{AppHandle, Manager, Theme};

    /// macOS / Windows は tao の値がそのまま使える。
    /// 変化は `WindowEvent::ThemeChanged` で届くので、ここでは初期値だけ返す。
    pub fn watch(app: AppHandle) -> bool {
        app.get_webview_window("main")
            .and_then(|w| w.theme().ok())
            .is_some_and(|theme| matches!(theme, Theme::Dark))
    }
}
