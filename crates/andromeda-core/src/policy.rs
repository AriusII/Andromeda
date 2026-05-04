use crate::{AndromedaError, AndromedaErrorKind, AndromedaResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareArchitecture {
    X64,
    Arm64,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CpuCapabilityClass {
    Conservative,
    Scalar64,
    Simd128,
    Simd256,
    Simd512,
}

impl CpuCapabilityClass {
    pub const fn supports_simd(self) -> bool {
        matches!(self, Self::Simd128 | Self::Simd256 | Self::Simd512)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuProfile {
    pub architecture: HardwareArchitecture,
    pub capability_class: CpuCapabilityClass,
    pub hardware_threads: u16,
}

impl CpuProfile {
    pub const fn conservative() -> Self {
        Self {
            architecture: HardwareArchitecture::Unknown,
            capability_class: CpuCapabilityClass::Conservative,
            hardware_threads: 1,
        }
    }

    pub const fn supports_simd(&self) -> bool {
        self.capability_class.supports_simd()
    }
}

impl Default for CpuProfile {
    fn default() -> Self {
        Self::conservative()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamSectionRole {
    Catalog,
    Execution,
    Cache,
    Temp,
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamSectionBudget {
    pub role: RamSectionRole,
    pub max_bytes: u64,
}

impl RamSectionBudget {
    pub const fn new(role: RamSectionRole, max_bytes: u64) -> Self {
        Self { role, max_bytes }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RamProfile {
    pub total_bytes: u64,
    pub sections: Vec<RamSectionBudget>,
}

impl RamProfile {
    pub const fn conservative() -> Self {
        Self {
            total_bytes: 0,
            sections: Vec::new(),
        }
    }

    pub fn new(total_bytes: u64, sections: Vec<RamSectionBudget>) -> Self {
        Self {
            total_bytes,
            sections,
        }
    }

    pub fn section_budget_bytes(&self, role: RamSectionRole) -> Option<u64> {
        self.sections
            .iter()
            .find(|section| section.role == role)
            .map(|section| section.max_bytes)
    }

    pub fn declared_section_bytes(&self) -> u64 {
        self.sections
            .iter()
            .map(|section| section.max_bytes)
            .fold(0_u64, u64::saturating_add)
    }

    pub fn validate_budgets(&self) -> AndromedaResult<()> {
        let declared = self.declared_section_bytes();
        if self.total_bytes != 0 && declared > self.total_bytes {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "RAM section budgets exceed declared total bytes",
            ));
        }

        Ok(())
    }
}

impl Default for RamProfile {
    fn default() -> Self {
        Self::conservative()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineClass {
    Commit,
    WalAppend,
    Rollback,
    Recovery,
    ForegroundExecution,
    BackgroundMaintenance,
    StatisticsRefresh,
    MapRefresh,
    BatchAnalytics,
}

impl PipelineClass {
    pub const fn is_critical_path(self) -> bool {
        matches!(
            self,
            Self::Commit | Self::WalAppend | Self::Rollback | Self::Recovery
        )
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::WalAppend => "wal_append",
            Self::Rollback => "rollback",
            Self::Recovery => "recovery",
            Self::ForegroundExecution => "foreground_execution",
            Self::BackgroundMaintenance => "background_maintenance",
            Self::StatisticsRefresh => "statistics_refresh",
            Self::MapRefresh => "map_refresh",
            Self::BatchAnalytics => "batch_analytics",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuExecutionPolicy {
    Disabled,
    OffCriticalPathOnly,
    BatchAnalyticsOnly,
}

impl GpuExecutionPolicy {
    pub const fn permits_pipeline(self, pipeline: PipelineClass) -> bool {
        match self {
            Self::Disabled => false,
            Self::OffCriticalPathOnly => !pipeline.is_critical_path(),
            Self::BatchAnalyticsOnly => matches!(
                pipeline,
                PipelineClass::StatisticsRefresh
                    | PipelineClass::MapRefresh
                    | PipelineClass::BatchAnalytics
            ),
        }
    }

    pub fn validate_pipeline(self, pipeline: PipelineClass) -> AndromedaResult<()> {
        if self.permits_pipeline(pipeline) {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            format!(
                "GPU execution is not permitted for {} pipeline",
                pipeline.name()
            ),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuProfile {
    pub available: bool,
    pub execution_policy: GpuExecutionPolicy,
}

impl GpuProfile {
    pub const fn disabled() -> Self {
        Self {
            available: false,
            execution_policy: GpuExecutionPolicy::Disabled,
        }
    }

    pub const fn off_critical_path() -> Self {
        Self {
            available: true,
            execution_policy: GpuExecutionPolicy::OffCriticalPathOnly,
        }
    }

    pub const fn batch_analytics_only() -> Self {
        Self {
            available: true,
            execution_policy: GpuExecutionPolicy::BatchAnalyticsOnly,
        }
    }

    pub fn validate_pipeline(&self, pipeline: PipelineClass) -> AndromedaResult<()> {
        if !self.available {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "GPU execution is not available",
            ));
        }

        self.execution_policy.validate_pipeline(pipeline)
    }
}

impl Default for GpuProfile {
    fn default() -> Self {
        Self::disabled()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareProfile {
    pub architecture: HardwareArchitecture,
    pub has_simd: bool,
    pub has_direct_io: bool,
    pub cpu: CpuProfile,
    pub ram: RamProfile,
    pub gpu: GpuProfile,
}

impl HardwareProfile {
    pub const fn conservative() -> Self {
        let cpu = CpuProfile::conservative();

        Self {
            architecture: cpu.architecture,
            has_simd: cpu.supports_simd(),
            has_direct_io: false,
            cpu,
            ram: RamProfile::conservative(),
            gpu: GpuProfile::disabled(),
        }
    }

    pub fn validate_gpu_pipeline(&self, pipeline: PipelineClass) -> AndromedaResult<()> {
        self.gpu.validate_pipeline(pipeline)
    }

    pub fn validate_ram_budgets(&self) -> AndromedaResult<()> {
        self.ram.validate_budgets()
    }
}

impl Default for HardwareProfile {
    fn default() -> Self {
        Self::conservative()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceBudget {
    pub max_memory_bytes: u64,
    pub max_temp_bytes: u64,
    pub max_streams: u32,
}

impl ResourceBudget {
    pub const fn new(max_memory_bytes: u64, max_temp_bytes: u64, max_streams: u32) -> Self {
        Self {
            max_memory_bytes,
            max_temp_bytes,
            max_streams,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conservative_profile_disables_gpu() {
        let profile = HardwareProfile::conservative();

        assert_eq!(
            profile.cpu.capability_class,
            CpuCapabilityClass::Conservative
        );
        assert!(!profile.has_simd);
        assert_eq!(profile.gpu.execution_policy, GpuExecutionPolicy::Disabled);
        assert!(profile
            .validate_gpu_pipeline(PipelineClass::BatchAnalytics)
            .is_err());
    }

    #[test]
    fn gpu_policy_rejects_commit_wal_rollback_and_recovery() {
        let policy = GpuExecutionPolicy::OffCriticalPathOnly;

        for pipeline in [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
        ] {
            let error = policy.validate_pipeline(pipeline).unwrap_err();
            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
            assert!(!policy.permits_pipeline(pipeline));
        }
    }

    #[test]
    fn batch_analytics_policy_only_allows_analytical_gpu_work() {
        let policy = GpuExecutionPolicy::BatchAnalyticsOnly;

        for pipeline in [
            PipelineClass::StatisticsRefresh,
            PipelineClass::MapRefresh,
            PipelineClass::BatchAnalytics,
        ] {
            assert!(policy.validate_pipeline(pipeline).is_ok());
        }

        for pipeline in [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
            PipelineClass::ForegroundExecution,
            PipelineClass::BackgroundMaintenance,
        ] {
            assert!(policy.validate_pipeline(pipeline).is_err());
        }
    }

    #[test]
    fn gpu_batch_analytics_profile_rejects_foreground_and_critical_work() {
        let profile = GpuProfile::batch_analytics_only();

        assert!(profile
            .validate_pipeline(PipelineClass::StatisticsRefresh)
            .is_ok());
        assert!(profile.validate_pipeline(PipelineClass::MapRefresh).is_ok());
        assert!(profile
            .validate_pipeline(PipelineClass::BatchAnalytics)
            .is_ok());

        for pipeline in [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
            PipelineClass::ForegroundExecution,
            PipelineClass::BackgroundMaintenance,
        ] {
            assert!(profile.validate_pipeline(pipeline).is_err());
        }
    }

    #[test]
    fn ram_profile_validates_section_budgets() {
        let profile = RamProfile::new(
            128,
            vec![
                RamSectionBudget::new(RamSectionRole::Execution, 64),
                RamSectionBudget::new(RamSectionRole::Temp, 32),
            ],
        );

        assert_eq!(
            profile.section_budget_bytes(RamSectionRole::Execution),
            Some(64)
        );
        assert_eq!(profile.section_budget_bytes(RamSectionRole::Cache), None);
        assert!(profile.validate_budgets().is_ok());

        let over_budget = RamProfile::new(
            8,
            vec![
                RamSectionBudget::new(RamSectionRole::Execution, 8),
                RamSectionBudget::new(RamSectionRole::Temp, 8),
            ],
        );

        assert!(over_budget.validate_budgets().is_err());
    }
}
