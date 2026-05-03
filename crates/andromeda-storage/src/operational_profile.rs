use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CpuCapabilityClass, CpuProfile,
    GpuExecutionPolicy, GpuProfile, HardwareArchitecture, HardwareProfile, PipelineClass,
    RamProfile, RamSectionBudget, RamSectionRole,
};

use crate::{
    HotColdIoThresholds, IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget,
    IoUseClass, PageIoBudget, PageSize, SegmentIoBudget,
};

const MIB: u64 = 1024 * 1024;
const CONSERVATIVE_RAM_BYTES: u64 = 256 * MIB;
const HOT_WRITE_RAM_BYTES: u64 = 512 * MIB;
const COLD_ARCHIVE_RAM_BYTES: u64 = 384 * MIB;
const ANALYTICS_RAM_BYTES: u64 = 768 * MIB;
const HOT_SEGMENT_BYTES: u64 = 64 * MIB;
const COLD_SEGMENT_BYTES: u64 = 128 * MIB;

/// Named operational IO presets for storage workflow validation.
///
/// Presets are guardrail contracts only. They do not report benchmarks or make
/// claims about device-specific throughput or latency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalProfileMode {
    Conservative,
    HotWrite,
    ColdArchive,
    AnalyticsOffCriticalPath,
}

/// Storage IO workflow section of an operational preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoWorkflowProfile {
    pub mode: OperationalProfileMode,
    pub page_budget: PageIoBudget,
    pub segment_budget: SegmentIoBudget,
    pub thresholds: HotColdIoThresholds,
}

impl IoWorkflowProfile {
    pub const fn conservative() -> Self {
        let thresholds = HotColdIoThresholds::conservative();
        Self {
            mode: OperationalProfileMode::Conservative,
            page_budget: hot_page_budget(5_000, 5_000, 10_000, 64 * MIB),
            segment_budget: hot_segment_budget(
                HOT_SEGMENT_BYTES,
                5_000_000,
                5_000_000,
                10_000,
                64 * MIB,
                thresholds,
            ),
            thresholds,
        }
    }

    pub const fn hot_write() -> Self {
        let thresholds = HotColdIoThresholds::conservative();
        Self {
            mode: OperationalProfileMode::HotWrite,
            page_budget: hot_page_budget(2_000, 2_000, 5_000, 128 * MIB),
            segment_budget: hot_segment_budget(
                HOT_SEGMENT_BYTES,
                2_000_000,
                2_000_000,
                5_000,
                128 * MIB,
                thresholds,
            ),
            thresholds,
        }
    }

    pub const fn cold_archive() -> Self {
        let thresholds = HotColdIoThresholds::conservative();
        Self {
            mode: OperationalProfileMode::ColdArchive,
            page_budget: hot_page_budget(10_000, 10_000, 10_000, 64 * MIB),
            segment_budget: cold_segment_budget(
                COLD_SEGMENT_BYTES,
                5_000_000,
                5_000_000,
                5_000_000,
                64 * MIB,
                thresholds,
            ),
            thresholds,
        }
    }

    pub const fn analytics_off_critical_path() -> Self {
        let thresholds = HotColdIoThresholds::conservative();
        Self {
            mode: OperationalProfileMode::AnalyticsOffCriticalPath,
            page_budget: hot_page_budget(10_000, 10_000, 10_000, 64 * MIB),
            segment_budget: cold_segment_budget(
                COLD_SEGMENT_BYTES,
                5_000_000,
                5_000_000,
                5_000_000,
                64 * MIB,
                thresholds,
            ),
            thresholds,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.thresholds.validate()?;
        if self.segment_budget.thresholds != self.thresholds {
            return Err(storage_error(
                "workflow thresholds must match the segment IO budget thresholds",
            ));
        }

        self.page_budget.validate()?;
        self.segment_budget.validate()?;

        match self.mode {
            OperationalProfileMode::Conservative | OperationalProfileMode::HotWrite => {
                self.require_hot_page_path()?;
                self.require_hot_segment_path()?;
            }
            OperationalProfileMode::ColdArchive
            | OperationalProfileMode::AnalyticsOffCriticalPath => {
                self.require_hot_page_path()?;
                self.require_cold_segment_path()?;
            }
        }

        Ok(())
    }

    fn require_hot_page_path(&self) -> AndromedaResult<()> {
        if self.page_budget.use_class != IoUseClass::OnlineHotPath
            || self.page_budget.path_budget.path_class != IoPathClass::HotPathNvmeSsd
        {
            return Err(storage_error(
                "workflow page budget must keep online pages on the HotStore path",
            ));
        }
        Ok(())
    }

    fn require_hot_segment_path(&self) -> AndromedaResult<()> {
        if self.segment_budget.use_class != IoUseClass::CommitCriticalHotPath
            || self.segment_budget.path_budget.path_class != IoPathClass::HotPathNvmeSsd
        {
            return Err(storage_error(
                "workflow segment budget must keep commit-critical writes on HotStore",
            ));
        }
        Ok(())
    }

    fn require_cold_segment_path(&self) -> AndromedaResult<()> {
        if self.segment_budget.use_class != IoUseClass::ColdSegmentPath
            || self.segment_budget.path_budget.path_class != IoPathClass::ColdPathHdd
        {
            return Err(storage_error(
                "workflow segment budget must use the ColdStore path for archive work",
            ));
        }
        Ok(())
    }
}

impl Default for IoWorkflowProfile {
    fn default() -> Self {
        Self::conservative()
    }
}

/// Full operational profile binding CPU, RAM, storage IO, and accelerator use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalProfile {
    pub mode: OperationalProfileMode,
    pub hardware: HardwareProfile,
    pub workflow: IoWorkflowProfile,
}

impl OperationalProfile {
    pub fn conservative() -> Self {
        Self::new(
            OperationalProfileMode::Conservative,
            conservative_hardware(CONSERVATIVE_RAM_BYTES),
            IoWorkflowProfile::conservative(),
        )
    }

    pub fn hot_write() -> Self {
        Self::new(
            OperationalProfileMode::HotWrite,
            hot_write_hardware(),
            IoWorkflowProfile::hot_write(),
        )
    }

    pub fn cold_archive() -> Self {
        Self::new(
            OperationalProfileMode::ColdArchive,
            conservative_hardware(COLD_ARCHIVE_RAM_BYTES),
            IoWorkflowProfile::cold_archive(),
        )
    }

    pub fn analytics_off_critical_path() -> Self {
        Self::new(
            OperationalProfileMode::AnalyticsOffCriticalPath,
            analytics_hardware(),
            IoWorkflowProfile::analytics_off_critical_path(),
        )
    }

    pub fn new(
        mode: OperationalProfileMode,
        hardware: HardwareProfile,
        workflow: IoWorkflowProfile,
    ) -> Self {
        Self {
            mode,
            hardware,
            workflow,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.workflow.mode != self.mode {
            return Err(storage_error(
                "operational profile mode must match the workflow mode",
            ));
        }

        self.hardware.validate_ram_budgets()?;
        self.workflow.validate()?;
        self.validate_gpu_policy()?;

        Ok(())
    }

    pub fn validate_gpu_pipeline(&self, pipeline: PipelineClass) -> AndromedaResult<()> {
        self.hardware.validate_gpu_pipeline(pipeline)
    }

    fn validate_gpu_policy(&self) -> AndromedaResult<()> {
        for pipeline in [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
        ] {
            if self
                .hardware
                .gpu
                .execution_policy
                .permits_pipeline(pipeline)
            {
                return Err(resource_error(
                    "operational profiles must not permit GPU work on critical pipelines",
                ));
            }
        }

        match self.mode {
            OperationalProfileMode::AnalyticsOffCriticalPath => {
                if !self.hardware.gpu.available
                    || self.hardware.gpu.execution_policy != GpuExecutionPolicy::BatchAnalyticsOnly
                {
                    return Err(resource_error(
                        "analytics profile requires GPU access limited to analytics pipelines",
                    ));
                }
            }
            OperationalProfileMode::Conservative
            | OperationalProfileMode::HotWrite
            | OperationalProfileMode::ColdArchive => {
                if self.hardware.gpu != GpuProfile::disabled() {
                    return Err(resource_error(
                        "non-analytics operational profiles must keep GPU execution disabled",
                    ));
                }
            }
        }

        Ok(())
    }
}

impl Default for OperationalProfile {
    fn default() -> Self {
        Self::conservative()
    }
}

const fn hot_page_budget(
    read_us: u64,
    write_us: u64,
    flush_us: u64,
    throughput_bytes_per_sec: u64,
) -> PageIoBudget {
    PageIoBudget::new(
        PageSize::KiB16,
        IoUseClass::OnlineHotPath,
        IoPathBudget::new(
            IoPathClass::HotPathNvmeSsd,
            IoLatencyBudget::new(read_us, write_us, flush_us),
            IoThroughputBudget::new(throughput_bytes_per_sec, throughput_bytes_per_sec),
        ),
    )
}

const fn hot_segment_budget(
    segment_bytes: u64,
    read_us: u64,
    write_us: u64,
    flush_us: u64,
    throughput_bytes_per_sec: u64,
    thresholds: HotColdIoThresholds,
) -> SegmentIoBudget {
    SegmentIoBudget::new(
        segment_bytes,
        IoUseClass::CommitCriticalHotPath,
        IoPathBudget::new(
            IoPathClass::HotPathNvmeSsd,
            IoLatencyBudget::new(read_us, write_us, flush_us),
            IoThroughputBudget::new(throughput_bytes_per_sec, throughput_bytes_per_sec),
        ),
        thresholds,
    )
}

const fn cold_segment_budget(
    segment_bytes: u64,
    read_us: u64,
    write_us: u64,
    flush_us: u64,
    throughput_bytes_per_sec: u64,
    thresholds: HotColdIoThresholds,
) -> SegmentIoBudget {
    SegmentIoBudget::new(
        segment_bytes,
        IoUseClass::ColdSegmentPath,
        IoPathBudget::new(
            IoPathClass::ColdPathHdd,
            IoLatencyBudget::new(read_us, write_us, flush_us),
            IoThroughputBudget::new(throughput_bytes_per_sec, throughput_bytes_per_sec),
        ),
        thresholds,
    )
}

fn conservative_hardware(total_ram_bytes: u64) -> HardwareProfile {
    let cpu = CpuProfile::conservative();
    HardwareProfile {
        architecture: cpu.architecture,
        has_simd: cpu.supports_simd(),
        has_direct_io: false,
        cpu,
        ram: ram_profile(
            total_ram_bytes,
            [
                (RamSectionRole::Catalog, total_ram_bytes / 8),
                (RamSectionRole::Execution, total_ram_bytes / 8),
                (RamSectionRole::Cache, total_ram_bytes / 4),
                (RamSectionRole::Temp, total_ram_bytes / 8),
                (RamSectionRole::Io, total_ram_bytes / 8),
            ],
        ),
        gpu: GpuProfile::disabled(),
    }
}

fn hot_write_hardware() -> HardwareProfile {
    let cpu = CpuProfile {
        architecture: HardwareArchitecture::Unknown,
        capability_class: CpuCapabilityClass::Scalar64,
        hardware_threads: 2,
    };
    HardwareProfile {
        architecture: cpu.architecture,
        has_simd: cpu.supports_simd(),
        has_direct_io: false,
        cpu,
        ram: ram_profile(
            HOT_WRITE_RAM_BYTES,
            [
                (RamSectionRole::Catalog, HOT_WRITE_RAM_BYTES / 8),
                (RamSectionRole::Execution, HOT_WRITE_RAM_BYTES / 4),
                (RamSectionRole::Cache, HOT_WRITE_RAM_BYTES / 4),
                (RamSectionRole::Temp, HOT_WRITE_RAM_BYTES / 8),
                (RamSectionRole::Io, HOT_WRITE_RAM_BYTES / 8),
            ],
        ),
        gpu: GpuProfile::disabled(),
    }
}

fn analytics_hardware() -> HardwareProfile {
    let cpu = CpuProfile {
        architecture: HardwareArchitecture::Unknown,
        capability_class: CpuCapabilityClass::Simd128,
        hardware_threads: 4,
    };
    HardwareProfile {
        architecture: cpu.architecture,
        has_simd: cpu.supports_simd(),
        has_direct_io: false,
        cpu,
        ram: ram_profile(
            ANALYTICS_RAM_BYTES,
            [
                (RamSectionRole::Catalog, ANALYTICS_RAM_BYTES / 8),
                (RamSectionRole::Execution, ANALYTICS_RAM_BYTES / 4),
                (RamSectionRole::Cache, ANALYTICS_RAM_BYTES / 4),
                (RamSectionRole::Temp, ANALYTICS_RAM_BYTES / 8),
                (RamSectionRole::Io, ANALYTICS_RAM_BYTES / 8),
            ],
        ),
        gpu: GpuProfile::batch_analytics_only(),
    }
}

fn ram_profile<const N: usize>(
    total_bytes: u64,
    sections: [(RamSectionRole, u64); N],
) -> RamProfile {
    RamProfile::new(
        total_bytes,
        sections
            .into_iter()
            .map(|(role, max_bytes)| RamSectionBudget::new(role, max_bytes))
            .collect(),
    )
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

fn resource_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Resource, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operational_presets_validate_as_contracts() {
        for profile in [
            OperationalProfile::conservative(),
            OperationalProfile::hot_write(),
            OperationalProfile::cold_archive(),
            OperationalProfile::analytics_off_critical_path(),
        ] {
            assert!(profile.validate().is_ok(), "{:?}", profile.mode);
        }
    }

    #[test]
    fn conservative_profile_uses_cpu_ram_and_hotstore_safe_defaults() {
        let profile = OperationalProfile::conservative();

        assert_eq!(
            profile.hardware.cpu.capability_class,
            CpuCapabilityClass::Conservative
        );
        assert_eq!(profile.hardware.gpu, GpuProfile::disabled());
        assert_eq!(profile.workflow.page_budget.page_size, PageSize::KiB16);
        assert_eq!(
            profile.workflow.page_budget.path_budget.path_class,
            IoPathClass::HotPathNvmeSsd
        );
        assert_eq!(
            profile.workflow.segment_budget.path_budget.path_class,
            IoPathClass::HotPathNvmeSsd
        );
        assert!(profile.hardware.ram.declared_section_bytes() <= profile.hardware.ram.total_bytes);
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn cold_archive_profile_uses_coldstore_only_for_segment_work() {
        let profile = OperationalProfile::cold_archive();

        assert_eq!(
            profile.workflow.page_budget.path_budget.path_class,
            IoPathClass::HotPathNvmeSsd
        );
        assert_eq!(
            profile.workflow.segment_budget.path_budget.path_class,
            IoPathClass::ColdPathHdd
        );
        assert_eq!(
            profile.workflow.segment_budget.use_class,
            IoUseClass::ColdSegmentPath
        );
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn analytics_gpu_profile_permits_only_off_critical_path_analytics_pipelines() {
        let profile = OperationalProfile::analytics_off_critical_path();

        for pipeline in [
            PipelineClass::StatisticsRefresh,
            PipelineClass::MapRefresh,
            PipelineClass::BatchAnalytics,
        ] {
            assert!(profile.validate_gpu_pipeline(pipeline).is_ok());
        }

        for pipeline in [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
            PipelineClass::ForegroundExecution,
            PipelineClass::BackgroundMaintenance,
        ] {
            assert!(profile.validate_gpu_pipeline(pipeline).is_err());
        }

        assert!(profile.validate().is_ok());
    }

    #[test]
    fn validation_rejects_mode_or_threshold_drift() {
        let mut profile = OperationalProfile::hot_write();
        profile.workflow.mode = OperationalProfileMode::Conservative;
        assert_eq!(
            profile.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );

        let mut workflow = IoWorkflowProfile::cold_archive();
        workflow.thresholds = HotColdIoThresholds::new(1, 1);
        assert_eq!(
            workflow.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
