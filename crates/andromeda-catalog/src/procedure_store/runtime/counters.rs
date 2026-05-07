#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcedureRuntimeCounters {
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

impl ProcedureRuntimeCounters {
    /// Backward-compatible minimal constructor.
    ///
    /// The richer counters default to zero and can be set with the `with_*`
    /// methods below. This lets execution wiring adopt the expanded Procedure
    /// Store contract incrementally without inventing values.
    pub const fn new(rows_read: u64, rows_written: u64, wal_bytes: u64, temp_bytes: u64) -> Self {
        Self {
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
        self.rows_read == 0
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
