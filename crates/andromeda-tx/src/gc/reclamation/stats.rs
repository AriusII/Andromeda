use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Statistics for reclamation marking and processing.
#[derive(Debug, Clone)]
pub struct ReclamationStats {
    marks_created: Arc<AtomicU64>,
    commands_executed: Arc<AtomicU64>,
    versions_reclaimed: Arc<AtomicU64>,
}

impl ReclamationStats {
    pub fn new() -> Self {
        ReclamationStats {
            marks_created: Arc::new(AtomicU64::new(0)),
            commands_executed: Arc::new(AtomicU64::new(0)),
            versions_reclaimed: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn marks_created(&self) -> u64 {
        self.marks_created.load(Ordering::Relaxed)
    }

    pub fn commands_executed(&self) -> u64 {
        self.commands_executed.load(Ordering::Relaxed)
    }

    pub fn versions_reclaimed(&self) -> u64 {
        self.versions_reclaimed.load(Ordering::Relaxed)
    }

    pub fn record_mark(&self) {
        self.marks_created.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_execution(&self) {
        self.commands_executed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_reclamation(&self) {
        self.versions_reclaimed.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for ReclamationStats {
    fn default() -> Self {
        Self::new()
    }
}
