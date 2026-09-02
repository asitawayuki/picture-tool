use crate::security::WritableRoots;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub struct ProcessingState {
    pub cancel_flag: Arc<AtomicBool>,
    pub thumbnail_cache: Mutex<LruCache<String, String>>,
    /// ネイティブダイアログで書き込みを許可されたフォルダー。
    /// 起動時は空＝どこにも書けない状態から始まる。
    pub writable_roots: WritableRoots,
    /// メタデータの下書きに未保存の変更があるか。
    /// フロントが `set_unsaved_state` で更新し、終了時の確認だけがこれを読む
    pub unsaved: AtomicBool,
    /// 終了確認のダイアログを表示中か。
    /// **表示中にもう一度 `CloseRequested` が来うる**ので抑止に使う（刷新 spec §3-4）
    pub close_prompt_open: AtomicBool,
}

impl ProcessingState {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
            thumbnail_cache: Mutex::new(LruCache::new(NonZeroUsize::new(500).unwrap())),
            writable_roots: WritableRoots::default(),
            unsaved: AtomicBool::new(false),
            close_prompt_open: AtomicBool::new(false),
        }
    }
}

impl Default for ProcessingState {
    fn default() -> Self {
        Self::new()
    }
}
