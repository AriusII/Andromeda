use std::num::NonZeroU64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnalyticsWorkloadKind {
    StatisticsRefresh,
    MapRefresh,
    BenchmarkReview,
    OperationalDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnalyticsAccelerationPolicy {
    CpuOnly,
    OptionalAcceleratorWithCpuFallback,
}

impl AnalyticsAccelerationPolicy {
    pub const fn has_cpu_fallback(self) -> bool {
        matches!(
            self,
            Self::CpuOnly | Self::OptionalAcceleratorWithCpuFallback
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnalyticsExecutionBounds {
    pub max_input_rows: NonZeroU64,
    pub max_temp_bytes: NonZeroU64,
    pub cancellation_required: bool,
}

impl AnalyticsExecutionBounds {
    pub const fn new(
        max_input_rows: NonZeroU64,
        max_temp_bytes: NonZeroU64,
        cancellation_required: bool,
    ) -> Self {
        Self {
            max_input_rows,
            max_temp_bytes,
            cancellation_required,
        }
    }

    pub const fn is_bounded(self) -> bool {
        self.cancellation_required
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnalyticsJobDescriptor {
    pub workload_kind: AnalyticsWorkloadKind,
    pub bounds: AnalyticsExecutionBounds,
    pub acceleration_policy: AnalyticsAccelerationPolicy,
}

impl AnalyticsJobDescriptor {
    pub const fn new(
        workload_kind: AnalyticsWorkloadKind,
        bounds: AnalyticsExecutionBounds,
        acceleration_policy: AnalyticsAccelerationPolicy,
    ) -> Self {
        Self {
            workload_kind,
            bounds,
            acceleration_policy,
        }
    }
}

pub trait AdvisoryAnalyticsJob {
    fn workload_kind(&self) -> AnalyticsWorkloadKind;

    fn execution_bounds(&self) -> AnalyticsExecutionBounds;

    fn acceleration_policy(&self) -> AnalyticsAccelerationPolicy;

    fn is_source_truth(&self) -> bool {
        false
    }

    fn is_c5_critical_path(&self) -> bool {
        false
    }

    fn requires_decision_trace(&self) -> bool {
        true
    }

    fn is_admissible_advisory_job(&self) -> bool {
        self.execution_bounds().is_bounded()
            && self.acceleration_policy().has_cpu_fallback()
            && !self.is_source_truth()
            && !self.is_c5_critical_path()
    }
}

impl AdvisoryAnalyticsJob for AnalyticsJobDescriptor {
    fn workload_kind(&self) -> AnalyticsWorkloadKind {
        self.workload_kind
    }

    fn execution_bounds(&self) -> AnalyticsExecutionBounds {
        self.bounds
    }

    fn acceleration_policy(&self) -> AnalyticsAccelerationPolicy {
        self.acceleration_policy
    }
}
