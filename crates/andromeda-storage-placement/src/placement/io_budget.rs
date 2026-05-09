use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_storage_page::PageSize;

/// Stable storage IO lane classes used by layout validation.
///
/// These are validation contracts for placement and budget sanity. They are not
/// benchmark results and do not claim any device-specific performance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoPathClass {
    /// CPU/RAM resident path, for page-cache or in-memory staging work.
    CpuRam,
    /// Commit-capable hot path backed by NVMe or SSD media.
    HotPathNvmeSsd,
    /// Capacity-oriented cold path backed by HDD media.
    ColdPathHdd,
}

/// Logical IO use-site for validating lane assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoUseClass {
    /// Synchronous commit, WAL, or other hot-path durability work.
    CommitCriticalHotPath,
    /// Online page read/write path where cold media must not be selected.
    OnlineHotPath,
    /// Cold immutable segment publication or scan path.
    ColdSegmentPath,
    /// Background work that is not on the commit-critical path.
    Background,
}

impl IoUseClass {
    pub const fn allows_path(self, path: IoPathClass) -> bool {
        match self {
            Self::CommitCriticalHotPath => matches!(path, IoPathClass::HotPathNvmeSsd),
            Self::OnlineHotPath => !matches!(path, IoPathClass::ColdPathHdd),
            Self::ColdSegmentPath | Self::Background => true,
        }
    }
}

/// Latency ceilings in microseconds for a budgeted IO lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoLatencyBudget {
    pub max_read_us: u64,
    pub max_write_us: u64,
    pub max_flush_us: u64,
}

impl IoLatencyBudget {
    pub const fn new(max_read_us: u64, max_write_us: u64, max_flush_us: u64) -> Self {
        Self {
            max_read_us,
            max_write_us,
            max_flush_us,
        }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.max_read_us == 0 || self.max_write_us == 0 || self.max_flush_us == 0 {
            return Err(storage_error("IO latency budget values must not be zero"));
        }
        Ok(())
    }
}

/// Throughput floors in bytes per second for a budgeted IO lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoThroughputBudget {
    pub min_read_bytes_per_sec: u64,
    pub min_write_bytes_per_sec: u64,
}

impl IoThroughputBudget {
    pub const fn new(min_read_bytes_per_sec: u64, min_write_bytes_per_sec: u64) -> Self {
        Self {
            min_read_bytes_per_sec,
            min_write_bytes_per_sec,
        }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.min_read_bytes_per_sec == 0 || self.min_write_bytes_per_sec == 0 {
            return Err(storage_error(
                "IO throughput budget values must not be zero",
            ));
        }
        Ok(())
    }
}

/// Combined latency and throughput validation contract for an IO lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoPathBudget {
    pub path_class: IoPathClass,
    pub latency: IoLatencyBudget,
    pub throughput: IoThroughputBudget,
}

impl IoPathBudget {
    pub const fn new(
        path_class: IoPathClass,
        latency: IoLatencyBudget,
        throughput: IoThroughputBudget,
    ) -> Self {
        Self {
            path_class,
            latency,
            throughput,
        }
    }

    pub fn validate_for_use(self, use_class: IoUseClass) -> AndromedaResult<()> {
        self.latency.validate()?;
        self.throughput.validate()?;
        if !use_class.allows_path(self.path_class) {
            return Err(storage_error(
                "IO path class is not permitted for the requested use class",
            ));
        }
        Ok(())
    }
}

/// Policy thresholds that separate hot placement from cold segment placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotColdIoThresholds {
    /// Maximum commit-critical flush latency contract for a hot lane.
    pub hot_commit_max_flush_us: u64,
    /// Minimum segment size for cold-path placement consideration.
    pub cold_segment_min_bytes: u64,
}

impl HotColdIoThresholds {
    pub const fn new(hot_commit_max_flush_us: u64, cold_segment_min_bytes: u64) -> Self {
        Self {
            hot_commit_max_flush_us,
            cold_segment_min_bytes,
        }
    }

    /// Conservative contract thresholds. These are guardrails, not benchmark
    /// measurements for any specific device.
    pub const fn conservative() -> Self {
        Self {
            hot_commit_max_flush_us: 10_000,
            cold_segment_min_bytes: 64 * 1024 * 1024,
        }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.hot_commit_max_flush_us == 0 || self.cold_segment_min_bytes == 0 {
            return Err(storage_error("hot/cold IO thresholds must not be zero"));
        }
        Ok(())
    }
}

impl Default for HotColdIoThresholds {
    fn default() -> Self {
        Self::conservative()
    }
}

/// Page-level IO budget validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageIoBudget {
    pub page_size: PageSize,
    pub use_class: IoUseClass,
    pub path_budget: IoPathBudget,
}

impl PageIoBudget {
    pub const fn new(
        page_size: PageSize,
        use_class: IoUseClass,
        path_budget: IoPathBudget,
    ) -> Self {
        Self {
            page_size,
            use_class,
            path_budget,
        }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        self.path_budget.validate_for_use(self.use_class)?;
        let page_bytes = u64::from(self.page_size.bytes());
        ensure_transfer_possible(
            page_bytes,
            self.path_budget.throughput.min_read_bytes_per_sec,
            self.path_budget.latency.max_read_us,
            "page read budget cannot transfer one page within the read latency ceiling",
        )?;
        ensure_transfer_possible(
            page_bytes,
            self.path_budget.throughput.min_write_bytes_per_sec,
            self.path_budget.latency.max_write_us,
            "page write budget cannot transfer one page within the write latency ceiling",
        )?;
        Ok(())
    }
}

/// Segment-level IO budget validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentIoBudget {
    pub segment_bytes: u64,
    pub use_class: IoUseClass,
    pub path_budget: IoPathBudget,
    pub thresholds: HotColdIoThresholds,
}

impl SegmentIoBudget {
    pub const fn new(
        segment_bytes: u64,
        use_class: IoUseClass,
        path_budget: IoPathBudget,
        thresholds: HotColdIoThresholds,
    ) -> Self {
        Self {
            segment_bytes,
            use_class,
            path_budget,
            thresholds,
        }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        self.path_budget.validate_for_use(self.use_class)?;
        self.thresholds.validate()?;
        if self.segment_bytes == 0 {
            return Err(storage_error("segment IO budget size must not be zero"));
        }
        if self.use_class == IoUseClass::CommitCriticalHotPath
            && self.path_budget.latency.max_flush_us > self.thresholds.hot_commit_max_flush_us
        {
            return Err(storage_error(
                "commit-critical hot path flush latency exceeds hot threshold",
            ));
        }
        if self.path_budget.path_class == IoPathClass::ColdPathHdd
            && self.segment_bytes < self.thresholds.cold_segment_min_bytes
        {
            return Err(storage_error(
                "cold HDD segment placement requires the cold segment size threshold",
            ));
        }
        ensure_transfer_possible(
            self.segment_bytes,
            self.path_budget.throughput.min_read_bytes_per_sec,
            self.path_budget.latency.max_read_us,
            "segment read budget cannot transfer the segment within the read latency ceiling",
        )?;
        ensure_transfer_possible(
            self.segment_bytes,
            self.path_budget.throughput.min_write_bytes_per_sec,
            self.path_budget.latency.max_write_us,
            "segment write budget cannot transfer the segment within the write latency ceiling",
        )?;
        Ok(())
    }
}

fn ensure_transfer_possible(
    bytes: u64,
    bytes_per_sec: u64,
    latency_us: u64,
    message: &'static str,
) -> AndromedaResult<()> {
    let budgeted_bytes = u128::from(bytes_per_sec) * u128::from(latency_us);
    let required_byte_micros = u128::from(bytes) * 1_000_000;
    if budgeted_bytes < required_byte_micros {
        return Err(storage_error(message));
    }
    Ok(())
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hot_budget() -> IoPathBudget {
        IoPathBudget::new(
            IoPathClass::HotPathNvmeSsd,
            IoLatencyBudget::new(1_000, 1_000, 2_000),
            IoThroughputBudget::new(64 * 1024 * 1024, 64 * 1024 * 1024),
        )
    }

    fn cold_budget() -> IoPathBudget {
        IoPathBudget::new(
            IoPathClass::ColdPathHdd,
            IoLatencyBudget::new(5_000_000, 5_000_000, 5_000_000),
            IoThroughputBudget::new(64 * 1024 * 1024, 64 * 1024 * 1024),
        )
    }

    #[test]
    fn page_budget_accepts_valid_hot_page_contract() {
        let budget = PageIoBudget::new(PageSize::KiB16, IoUseClass::OnlineHotPath, hot_budget());

        assert!(budget.validate().is_ok());
    }

    #[test]
    fn io_budget_rejects_zero_values() {
        let budget = IoPathBudget::new(
            IoPathClass::HotPathNvmeSsd,
            IoLatencyBudget::new(0, 1, 1),
            IoThroughputBudget::new(1, 1),
        );

        assert_eq!(
            budget
                .validate_for_use(IoUseClass::CommitCriticalHotPath)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );

        let budget = IoPathBudget::new(
            IoPathClass::HotPathNvmeSsd,
            IoLatencyBudget::new(1, 1, 1),
            IoThroughputBudget::new(0, 1),
        );

        assert_eq!(
            budget
                .validate_for_use(IoUseClass::CommitCriticalHotPath)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn page_budget_rejects_impossible_transfer_contract() {
        let budget = IoPathBudget::new(
            IoPathClass::HotPathNvmeSsd,
            IoLatencyBudget::new(1, 1, 1),
            IoThroughputBudget::new(1, 1),
        );
        let page_budget =
            PageIoBudget::new(PageSize::KiB16, IoUseClass::CommitCriticalHotPath, budget);

        assert_eq!(
            page_budget.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn segment_budget_rejects_cold_hdd_for_commit_critical_hot_path() {
        let segment_budget = SegmentIoBudget::new(
            128 * 1024 * 1024,
            IoUseClass::CommitCriticalHotPath,
            cold_budget(),
            HotColdIoThresholds::conservative(),
        );

        assert_eq!(
            segment_budget.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn segment_budget_enforces_hot_and_cold_thresholds() {
        let slow_flush_hot_budget = IoPathBudget::new(
            IoPathClass::HotPathNvmeSsd,
            IoLatencyBudget::new(5_000_000, 5_000_000, 20_000),
            IoThroughputBudget::new(64 * 1024 * 1024, 64 * 1024 * 1024),
        );
        let segment_budget = SegmentIoBudget::new(
            128 * 1024 * 1024,
            IoUseClass::CommitCriticalHotPath,
            slow_flush_hot_budget,
            HotColdIoThresholds::conservative(),
        );

        assert_eq!(
            segment_budget.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );

        let segment_budget = SegmentIoBudget::new(
            16 * 1024 * 1024,
            IoUseClass::ColdSegmentPath,
            cold_budget(),
            HotColdIoThresholds::conservative(),
        );

        assert_eq!(
            segment_budget.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn segment_budget_accepts_valid_cold_segment_contract() {
        let segment_budget = SegmentIoBudget::new(
            128 * 1024 * 1024,
            IoUseClass::ColdSegmentPath,
            cold_budget(),
            HotColdIoThresholds::conservative(),
        );

        assert!(segment_budget.validate().is_ok());
    }
}
