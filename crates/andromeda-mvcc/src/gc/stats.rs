use std::sync::atomic::{AtomicU64, Ordering};

/// Statistics for garbage collection runs.
#[derive(Debug)]
pub struct GcStats {
    versions_scanned: AtomicU64,
    versions_reclaimed: AtomicU64,
    runs: AtomicU64,
    last_run_ms: AtomicU64, // Wall-clock timestamp (ms) of last GC run
}

impl GcStats {
    pub fn new() -> Self {
        GcStats {
            versions_scanned: AtomicU64::new(0),
            versions_reclaimed: AtomicU64::new(0),
            runs: AtomicU64::new(0),
            last_run_ms: AtomicU64::new(0),
        }
    }

    pub fn versions_scanned(&self) -> u64 {
        self.versions_scanned.load(Ordering::Relaxed)
    }

    pub fn versions_reclaimed(&self) -> u64 {
        self.versions_reclaimed.load(Ordering::Relaxed)
    }

    pub fn runs(&self) -> u64 {
        self.runs.load(Ordering::Relaxed)
    }

    pub fn last_run_ms(&self) -> u64 {
        self.last_run_ms.load(Ordering::Relaxed)
    }

    pub(super) fn record_scan(&self, count: u64) {
        self.versions_scanned.fetch_add(count, Ordering::Relaxed);
    }

    pub(super) fn record_reclaim(&self, count: u64) {
        self.versions_reclaimed.fetch_add(count, Ordering::Relaxed);
    }

    pub(super) fn record_run(&self, now_ms: u64) {
        self.runs.fetch_add(1, Ordering::Relaxed);
        self.last_run_ms.store(now_ms, Ordering::Relaxed);
    }
}

impl Default for GcStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of GC statistics at a point in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcStatSnapshot {
    pub versions_scanned: u64,
    pub versions_reclaimed: u64,
    pub runs: u64,
    pub last_run_ms: u64,
}
