use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[derive(Debug, Clone)]
pub struct ResultStreamMetrics {
    pub queue_depth: usize,
    pub total_rows_pushed: u64,
    pub total_rows_consumed: u64,
    pub backpressure_count: u64,
    pub peak_queue_depth: usize,
    pub memory_usage_bytes: u64,
}

pub(crate) struct ResultStreamMetricsInner {
    queue_depth: AtomicUsize,
    total_rows_pushed: AtomicU64,
    total_rows_consumed: AtomicU64,
    backpressure_count: AtomicU64,
    peak_queue_depth: AtomicUsize,
    memory_usage_bytes: AtomicU64,
}

impl ResultStreamMetricsInner {
    pub(crate) fn new() -> Self {
        Self {
            queue_depth: AtomicUsize::new(0),
            total_rows_pushed: AtomicU64::new(0),
            total_rows_consumed: AtomicU64::new(0),
            backpressure_count: AtomicU64::new(0),
            peak_queue_depth: AtomicUsize::new(0),
            memory_usage_bytes: AtomicU64::new(0),
        }
    }

    pub(crate) fn record_row_pushed(&self) {
        self.total_rows_pushed.fetch_add(1, Ordering::Relaxed);
        self.update_queue_depth();
    }

    pub(crate) fn record_row_consumed(&self) {
        self.total_rows_consumed.fetch_add(1, Ordering::Relaxed);
        self.update_queue_depth();
    }

    pub(crate) fn record_backpressure(&self) {
        self.backpressure_count.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn total_rows_pushed(&self) -> u64 {
        self.total_rows_pushed.load(Ordering::Acquire)
    }

    pub(crate) fn queue_depth(&self) -> usize {
        self.queue_depth.load(Ordering::Relaxed)
    }

    pub(crate) fn snapshot(&self) -> ResultStreamMetrics {
        ResultStreamMetrics {
            queue_depth: self.queue_depth.load(Ordering::Relaxed),
            total_rows_pushed: self.total_rows_pushed.load(Ordering::Relaxed),
            total_rows_consumed: self.total_rows_consumed.load(Ordering::Relaxed),
            backpressure_count: self.backpressure_count.load(Ordering::Relaxed),
            peak_queue_depth: self.peak_queue_depth.load(Ordering::Relaxed),
            memory_usage_bytes: self.memory_usage_bytes.load(Ordering::Relaxed),
        }
    }

    fn update_queue_depth(&self) {
        let pushed = self.total_rows_pushed.load(Ordering::Relaxed) as usize;
        let consumed = self.total_rows_consumed.load(Ordering::Relaxed) as usize;
        let depth = pushed.saturating_sub(consumed);

        self.queue_depth.store(depth, Ordering::Relaxed);

        let mut peak = self.peak_queue_depth.load(Ordering::Relaxed);
        while depth > peak {
            match self.peak_queue_depth.compare_exchange(
                peak,
                depth,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => peak = actual,
            }
        }
    }
}
