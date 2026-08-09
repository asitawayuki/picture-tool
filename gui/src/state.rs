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
}

impl ProcessingState {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
            thumbnail_cache: Mutex::new(LruCache::new(NonZeroUsize::new(500).unwrap())),
            writable_roots: WritableRoots::default(),
        }
    }
}

impl Default for ProcessingState {
    fn default() -> Self {
        Self::new()
    }
}
