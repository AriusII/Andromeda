#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvocationMetricKind {
    DurationMillis,
    RowsRead,
    RowsReturned,
    RowsWritten,
    RowsAffected,
    LogicalReads,
    PhysicalReads,
    CacheHits,
    WalBytes,
    TempBytes,
    SpillBytes,
}

impl InvocationMetricKind {
    pub const VARIANT_COUNT: usize = 11;

    pub const fn as_tag(self) -> u8 {
        match self {
            Self::DurationMillis => 0x01,
            Self::RowsRead => 0x02,
            Self::RowsReturned => 0x03,
            Self::RowsWritten => 0x04,
            Self::RowsAffected => 0x05,
            Self::LogicalReads => 0x06,
            Self::PhysicalReads => 0x07,
            Self::CacheHits => 0x08,
            Self::WalBytes => 0x09,
            Self::TempBytes => 0x0A,
            Self::SpillBytes => 0x0B,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct InvocationMetrics {
    pub duration_millis: u64,
    pub rows_read: u64,
    pub rows_returned: u64,
    pub rows_written: u64,
    pub rows_affected: u64,
    pub logical_reads: u64,
    pub physical_reads: u64,
    pub cache_hits: u64,
    pub wal_bytes: u64,
    pub temp_bytes: u64,
    pub spill_bytes: u64,
}

impl InvocationMetrics {
    pub const fn new(
        duration_millis: u64,
        rows_read: u64,
        rows_written: u64,
        wal_bytes: u64,
        temp_bytes: u64,
    ) -> Self {
        Self {
            duration_millis,
            rows_read,
            rows_returned: 0,
            rows_written,
            rows_affected: rows_written,
            logical_reads: 0,
            physical_reads: 0,
            cache_hits: 0,
            wal_bytes,
            temp_bytes,
            spill_bytes: 0,
        }
    }

    pub const fn with_rows_returned(mut self, rows_returned: u64) -> Self {
        self.rows_returned = rows_returned;
        self
    }

    pub const fn with_rows_affected(mut self, rows_affected: u64) -> Self {
        self.rows_affected = rows_affected;
        self
    }

    pub const fn with_read_counters(
        mut self,
        logical_reads: u64,
        physical_reads: u64,
        cache_hits: u64,
    ) -> Self {
        self.logical_reads = logical_reads;
        self.physical_reads = physical_reads;
        self.cache_hits = cache_hits;
        self
    }

    pub const fn with_spill_bytes(mut self, spill_bytes: u64) -> Self {
        self.spill_bytes = spill_bytes;
        self
    }

    pub const fn is_empty(self) -> bool {
        self.duration_millis == 0
            && self.rows_read == 0
            && self.rows_returned == 0
            && self.rows_written == 0
            && self.rows_affected == 0
            && self.logical_reads == 0
            && self.physical_reads == 0
            && self.cache_hits == 0
            && self.wal_bytes == 0
            && self.temp_bytes == 0
            && self.spill_bytes == 0
    }

    pub const fn has_rows_written(self) -> bool {
        self.rows_written > 0 || self.rows_affected > 0
    }

    pub const fn has_temp_or_spill_bytes(self) -> bool {
        self.temp_bytes > 0 || self.spill_bytes > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_track_runtime_counters_without_becoming_truth() {
        let metrics = InvocationMetrics::new(37, 10, 2, 128, 0)
            .with_rows_returned(8)
            .with_read_counters(20, 1, 19)
            .with_spill_bytes(4);

        assert_eq!(metrics.duration_millis, 37);
        assert_eq!(metrics.rows_read, 10);
        assert_eq!(metrics.rows_returned, 8);
        assert_eq!(metrics.rows_written, 2);
        assert_eq!(metrics.rows_affected, 2);
        assert_eq!(metrics.logical_reads, 20);
        assert_eq!(metrics.physical_reads, 1);
        assert_eq!(metrics.cache_hits, 19);
        assert_eq!(metrics.wal_bytes, 128);
        assert_eq!(metrics.spill_bytes, 4);
        assert!(metrics.has_rows_written());
        assert!(metrics.has_temp_or_spill_bytes());
        assert!(!metrics.is_empty());
    }

    #[test]
    fn metric_kind_tags_are_bounded() {
        assert_eq!(InvocationMetricKind::VARIANT_COUNT, 11);
        assert_eq!(InvocationMetricKind::DurationMillis.as_tag(), 0x01);
        assert_eq!(InvocationMetricKind::SpillBytes.as_tag(), 0x0B);
    }
}
