use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use lru::LruCache;
use std::num::NonZeroUsize;

pub struct ProcessingState {
    pub cancel_flag: Arc<AtomicBool>,
    pub thumbnail_cache: Mutex<LruCache<String, String>>,
}

impl ProcessingState {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
            thumbnail_cache: Mutex::new(
                LruCache::new(NonZeroUsize::new(500).unwrap())
            ),
        }
    }
}
