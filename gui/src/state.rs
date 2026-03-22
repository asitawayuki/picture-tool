use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct ProcessingState {
    pub cancel_flag: Arc<AtomicBool>,
}

impl ProcessingState {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}
