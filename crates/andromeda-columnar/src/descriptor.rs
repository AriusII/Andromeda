use std::num::{NonZeroU16, NonZeroU64};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnarConsumer {
    Statistics,
    Maps,
    Analytics,
    BenchmarkReview,
    OperationalDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnarAccelerationPolicy {
    CpuOnly,
    OptionalAcceleratorWithCpuFallback,
}

impl ColumnarAccelerationPolicy {
    pub const fn has_cpu_fallback(self) -> bool {
        matches!(
            self,
            Self::CpuOnly | Self::OptionalAcceleratorWithCpuFallback
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColumnarVersionBinding {
    pub catalog_version: NonZeroU64,
    pub storage_epoch: NonZeroU64,
    pub policy_version: NonZeroU64,
    pub statistics_version: Option<NonZeroU64>,
}

impl ColumnarVersionBinding {
    pub const fn new(
        catalog_version: NonZeroU64,
        storage_epoch: NonZeroU64,
        policy_version: NonZeroU64,
        statistics_version: Option<NonZeroU64>,
    ) -> Self {
        Self {
            catalog_version,
            storage_epoch,
            policy_version,
            statistics_version,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColumnarLayoutDescriptor {
    pub consumer: ColumnarConsumer,
    pub column_count: NonZeroU16,
    pub version_binding: ColumnarVersionBinding,
    pub acceleration_policy: ColumnarAccelerationPolicy,
}

impl ColumnarLayoutDescriptor {
    pub const fn new(
        consumer: ColumnarConsumer,
        column_count: NonZeroU16,
        version_binding: ColumnarVersionBinding,
        acceleration_policy: ColumnarAccelerationPolicy,
    ) -> Self {
        Self {
            consumer,
            column_count,
            version_binding,
            acceleration_policy,
        }
    }
}

pub trait ColumnarArtifactDescriptor {
    fn consumer(&self) -> ColumnarConsumer;

    fn column_count(&self) -> NonZeroU16;

    fn version_binding(&self) -> ColumnarVersionBinding;

    fn acceleration_policy(&self) -> ColumnarAccelerationPolicy;

    fn is_source_truth(&self) -> bool {
        false
    }

    fn is_c5_critical_path(&self) -> bool {
        false
    }

    fn requires_explicit_codec_before_persistence(&self) -> bool {
        true
    }

    fn is_advisory_columnar_artifact(&self) -> bool {
        self.acceleration_policy().has_cpu_fallback()
            && !self.is_source_truth()
            && !self.is_c5_critical_path()
    }
}

impl ColumnarArtifactDescriptor for ColumnarLayoutDescriptor {
    fn consumer(&self) -> ColumnarConsumer {
        self.consumer
    }

    fn column_count(&self) -> NonZeroU16 {
        self.column_count
    }

    fn version_binding(&self) -> ColumnarVersionBinding {
        self.version_binding
    }

    fn acceleration_policy(&self) -> ColumnarAccelerationPolicy {
        self.acceleration_policy
    }
}
