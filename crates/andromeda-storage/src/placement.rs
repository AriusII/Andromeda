use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, HardwareProfile, PipelineClass,
    RamSectionRole,
};

use crate::{
    HotColdIoThresholds, IoPathBudget, IoPathClass, IoUseClass, PageIoBudget, PageSize,
    SegmentDescriptor, SegmentIoBudget, SegmentMutation, SegmentState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataTemperature {
    RamWorkingSet,
    Hot,
    Cooling,
    Cold,
}

impl DataTemperature {
    pub const fn preferred_tier(self) -> StorageTier {
        match self {
            Self::RamWorkingSet => StorageTier::Ram,
            Self::Hot | Self::Cooling => StorageTier::HotStore,
            Self::Cold => StorageTier::ColdStore,
        }
    }

    pub const fn is_cold(self) -> bool {
        matches!(self, Self::Cold)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageTier {
    Ram,
    HotStore,
    ColdStore,
}

impl StorageTier {
    pub const fn media_class(self) -> &'static str {
        match self {
            Self::Ram => "volatile RAM working set",
            Self::HotStore => "NVMe/SSD HotStore",
            Self::ColdStore => "HDD ColdStore",
        }
    }

    pub const fn is_mutable(self) -> bool {
        !matches!(self, Self::ColdStore)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    BufferInRam,
    AppendHotStore,
    SealHotStoreSegment,
    PublishColdStore,
    ServeRead,
}

impl PipelineStage {
    pub const fn source_tier(self) -> Option<StorageTier> {
        match self {
            Self::BufferInRam => None,
            Self::AppendHotStore => Some(StorageTier::Ram),
            Self::SealHotStoreSegment => Some(StorageTier::HotStore),
            Self::PublishColdStore => Some(StorageTier::HotStore),
            Self::ServeRead => None,
        }
    }

    pub const fn destination_tier(self) -> Option<StorageTier> {
        match self {
            Self::BufferInRam => Some(StorageTier::Ram),
            Self::AppendHotStore | Self::SealHotStoreSegment => Some(StorageTier::HotStore),
            Self::PublishColdStore => Some(StorageTier::ColdStore),
            Self::ServeRead => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFallbackPolicy {
    RamThenHotThenCold,
    HotThenCold,
    ColdOnly,
}

impl ReadFallbackPolicy {
    pub const fn for_temperature(temperature: DataTemperature) -> Self {
        match temperature {
            DataTemperature::RamWorkingSet => Self::RamThenHotThenCold,
            DataTemperature::Hot | DataTemperature::Cooling => Self::HotThenCold,
            DataTemperature::Cold => Self::ColdOnly,
        }
    }

    pub const fn allows_tier(self, tier: StorageTier) -> bool {
        match self {
            Self::RamThenHotThenCold => true,
            Self::HotThenCold => matches!(tier, StorageTier::HotStore | StorageTier::ColdStore),
            Self::ColdOnly => matches!(tier, StorageTier::ColdStore),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementDecision {
    pub temperature: DataTemperature,
    pub target_tier: StorageTier,
    pub pipeline_stage: PipelineStage,
    pub read_fallback: ReadFallbackPolicy,
    pub mutation_allowed: bool,
    pub reason: &'static str,
}

impl PlacementDecision {
    pub const fn read(temperature: DataTemperature) -> Self {
        Self {
            temperature,
            target_tier: temperature.preferred_tier(),
            pipeline_stage: PipelineStage::ServeRead,
            read_fallback: ReadFallbackPolicy::for_temperature(temperature),
            mutation_allowed: false,
            reason: "read placement follows data temperature and explicit fallback policy",
        }
    }

    pub fn append(temperature: DataTemperature) -> AndromedaResult<Self> {
        if temperature.is_cold() {
            return Err(storage_error(
                "append placement cannot target ColdStore; cold data must remain immutable",
            ));
        }

        Ok(Self {
            temperature,
            target_tier: StorageTier::HotStore,
            pipeline_stage: PipelineStage::AppendHotStore,
            read_fallback: ReadFallbackPolicy::for_temperature(temperature),
            mutation_allowed: true,
            reason: "append enters the RAM to HotStore pipeline and never appends to ColdStore",
        })
    }

    pub fn mutation(
        descriptor: &SegmentDescriptor,
        mutation: SegmentMutation,
        temperature: DataTemperature,
    ) -> AndromedaResult<Self> {
        descriptor.validate_mutation(mutation)?;
        match mutation {
            SegmentMutation::AppendExtent => Self::append(temperature),
            SegmentMutation::UpdatePageInPlace | SegmentMutation::SplitSegment => {
                if descriptor.state == SegmentState::Sealed {
                    return Err(storage_error(
                        "sealed hot segment rejects update-in-place and split mutations",
                    ));
                }
                Ok(Self {
                    temperature,
                    target_tier: StorageTier::Ram,
                    pipeline_stage: PipelineStage::BufferInRam,
                    read_fallback: ReadFallbackPolicy::RamThenHotThenCold,
                    mutation_allowed: true,
                    reason: "mutable page work is staged in RAM before HotStore placement",
                })
            }
        }
    }

    pub fn seal_hot_segment(descriptor: &SegmentDescriptor) -> AndromedaResult<Self> {
        descriptor.validate()?;
        if descriptor.state != SegmentState::BuildingHotSnapshot {
            return Err(storage_error(
                "only a building hot snapshot segment can enter the HotStore seal stage",
            ));
        }

        Ok(Self {
            temperature: DataTemperature::Cooling,
            target_tier: StorageTier::HotStore,
            pipeline_stage: PipelineStage::SealHotStoreSegment,
            read_fallback: ReadFallbackPolicy::HotThenCold,
            mutation_allowed: false,
            reason: "sealing closes HotStore mutation before possible ColdStore publication",
        })
    }

    pub fn publish_cold_segment(descriptor: &SegmentDescriptor) -> AndromedaResult<Self> {
        descriptor.validate()?;
        if descriptor.state != SegmentState::Sealed {
            return Err(storage_error(
                "ColdStore publication requires a sealed HotStore segment descriptor",
            ));
        }
        if descriptor.snapshot_id.is_none() {
            return Err(storage_error(
                "ColdStore publication requires a nonzero snapshot reference",
            ));
        }

        Ok(Self {
            temperature: DataTemperature::Cold,
            target_tier: StorageTier::ColdStore,
            pipeline_stage: PipelineStage::PublishColdStore,
            read_fallback: ReadFallbackPolicy::ColdOnly,
            mutation_allowed: false,
            reason: "ColdStore publication is immutable and only follows HotStore sealing",
        })
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.target_tier == StorageTier::ColdStore && self.mutation_allowed {
            return Err(storage_error("ColdStore placement must not allow mutation"));
        }
        if self.pipeline_stage == PipelineStage::PublishColdStore
            && self.target_tier != StorageTier::ColdStore
        {
            return Err(storage_error(
                "ColdStore publication stage must target the ColdStore tier",
            ));
        }
        if self.pipeline_stage == PipelineStage::AppendHotStore
            && self.target_tier == StorageTier::ColdStore
        {
            return Err(storage_error("append stage must not target ColdStore"));
        }
        if !self.read_fallback.allows_tier(self.target_tier) {
            return Err(storage_error(
                "read fallback policy must include the placement target tier",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageWorkloadClass {
    Commit,
    WalAppend,
    Rollback,
    Recovery,
    RamWorkingSet,
    HotAppend,
    ColdRead,
    ColdPublication,
}

impl StorageWorkloadClass {
    pub const fn pipeline_class(self) -> PipelineClass {
        match self {
            Self::Commit => PipelineClass::Commit,
            Self::WalAppend => PipelineClass::WalAppend,
            Self::Rollback => PipelineClass::Rollback,
            Self::Recovery => PipelineClass::Recovery,
            Self::RamWorkingSet | Self::HotAppend | Self::ColdRead => {
                PipelineClass::ForegroundExecution
            }
            Self::ColdPublication => PipelineClass::BackgroundMaintenance,
        }
    }

    pub const fn io_use_class(self) -> IoUseClass {
        match self {
            Self::Commit | Self::WalAppend | Self::Rollback | Self::Recovery => {
                IoUseClass::CommitCriticalHotPath
            }
            Self::RamWorkingSet | Self::HotAppend => IoUseClass::OnlineHotPath,
            Self::ColdRead | Self::ColdPublication => IoUseClass::ColdSegmentPath,
        }
    }

    pub const fn ram_section_role(self) -> Option<RamSectionRole> {
        match self {
            Self::RamWorkingSet => Some(RamSectionRole::Cache),
            Self::Commit | Self::WalAppend | Self::HotAppend => Some(RamSectionRole::Io),
            Self::Rollback | Self::Recovery => Some(RamSectionRole::Execution),
            Self::ColdRead | Self::ColdPublication => None,
        }
    }

    fn placement_decision(self) -> AndromedaResult<PlacementDecision> {
        match self {
            Self::Commit | Self::WalAppend | Self::Rollback | Self::Recovery | Self::HotAppend => {
                PlacementDecision::append(DataTemperature::Hot)
            }
            Self::RamWorkingSet => Ok(PlacementDecision::read(DataTemperature::RamWorkingSet)),
            Self::ColdRead => Ok(PlacementDecision::read(DataTemperature::Cold)),
            Self::ColdPublication => Ok(PlacementDecision {
                temperature: DataTemperature::Cold,
                target_tier: StorageTier::ColdStore,
                pipeline_stage: PipelineStage::PublishColdStore,
                read_fallback: ReadFallbackPolicy::ColdOnly,
                mutation_allowed: false,
                reason: "policy maps cold publication to immutable ColdStore placement",
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageIoBudgetScope {
    Page(PageSize),
    Segment { bytes: u64 },
}

impl StorageIoBudgetScope {
    pub const fn logical_bytes(self) -> u64 {
        match self {
            Self::Page(page_size) => page_size.bytes() as u64,
            Self::Segment { bytes } => bytes,
        }
    }

    fn validate(
        self,
        use_class: IoUseClass,
        path_budget: IoPathBudget,
        thresholds: HotColdIoThresholds,
    ) -> AndromedaResult<()> {
        match self {
            Self::Page(page_size) => {
                PageIoBudget::new(page_size, use_class, path_budget).validate()
            }
            Self::Segment { bytes } => {
                SegmentIoBudget::new(bytes, use_class, path_budget, thresholds).validate()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreIoPlacementRequest {
    pub workload: StorageWorkloadClass,
    pub io_budget_scope: StorageIoBudgetScope,
    pub path_budget: IoPathBudget,
    pub use_gpu: bool,
}

impl CoreIoPlacementRequest {
    pub const fn new(
        workload: StorageWorkloadClass,
        io_budget_scope: StorageIoBudgetScope,
        path_budget: IoPathBudget,
        use_gpu: bool,
    ) -> Self {
        Self {
            workload,
            io_budget_scope,
            path_budget,
            use_gpu,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreIoPlacementDecision {
    pub workload: StorageWorkloadClass,
    pub placement: PlacementDecision,
    pub pipeline_class: PipelineClass,
    pub io_use_class: IoUseClass,
    pub path_budget: IoPathBudget,
    pub gpu_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreIoPlacementPolicy {
    pub hardware: HardwareProfile,
    pub thresholds: HotColdIoThresholds,
}

pub type StoragePlacementPolicy = CoreIoPlacementPolicy;

impl CoreIoPlacementPolicy {
    pub const fn new(hardware: HardwareProfile, thresholds: HotColdIoThresholds) -> Self {
        Self {
            hardware,
            thresholds,
        }
    }

    pub const fn conservative() -> Self {
        Self {
            hardware: HardwareProfile::conservative(),
            thresholds: HotColdIoThresholds::conservative(),
        }
    }

    pub fn plan(
        &self,
        request: CoreIoPlacementRequest,
    ) -> AndromedaResult<CoreIoPlacementDecision> {
        self.hardware.validate_ram_budgets()?;

        let pipeline_class = request.workload.pipeline_class();
        if request.use_gpu {
            self.hardware.validate_gpu_pipeline(pipeline_class)?;
        }

        let placement = request.workload.placement_decision()?;
        placement.validate()?;

        validate_core_placement_invariants(request.workload, &placement)?;
        validate_ram_budget_for_workload(
            &self.hardware,
            request.workload,
            request.io_budget_scope,
        )?;

        let expected_path = io_path_for_tier(placement.target_tier);
        if request.path_budget.path_class != expected_path {
            return Err(storage_error(
                "IO budget path class must match the selected storage placement tier",
            ));
        }

        let io_use_class = request.workload.io_use_class();
        request
            .io_budget_scope
            .validate(io_use_class, request.path_budget, self.thresholds)?;

        Ok(CoreIoPlacementDecision {
            workload: request.workload,
            placement,
            pipeline_class,
            io_use_class,
            path_budget: request.path_budget,
            gpu_enabled: request.use_gpu,
        })
    }
}

impl Default for CoreIoPlacementPolicy {
    fn default() -> Self {
        Self::conservative()
    }
}

fn validate_core_placement_invariants(
    workload: StorageWorkloadClass,
    placement: &PlacementDecision,
) -> AndromedaResult<()> {
    match workload {
        StorageWorkloadClass::Commit
        | StorageWorkloadClass::WalAppend
        | StorageWorkloadClass::Rollback
        | StorageWorkloadClass::Recovery
        | StorageWorkloadClass::HotAppend => {
            if placement.target_tier != StorageTier::HotStore {
                return Err(storage_error(
                    "commit-critical and hot append workloads must target HotStore",
                ));
            }
        }
        StorageWorkloadClass::RamWorkingSet => {
            if placement.target_tier != StorageTier::Ram {
                return Err(storage_error("RAM working set must target RAM placement"));
            }
        }
        StorageWorkloadClass::ColdRead | StorageWorkloadClass::ColdPublication => {
            if placement.target_tier != StorageTier::ColdStore {
                return Err(storage_error(
                    "cold reads and publication must target ColdStore",
                ));
            }
            if placement.mutation_allowed {
                return Err(storage_error(
                    "cold reads and publication must not allow mutation",
                ));
            }
        }
    }
    Ok(())
}

fn validate_ram_budget_for_workload(
    hardware: &HardwareProfile,
    workload: StorageWorkloadClass,
    io_budget_scope: StorageIoBudgetScope,
) -> AndromedaResult<()> {
    if let Some(role) = workload.ram_section_role() {
        let bytes = io_budget_scope.logical_bytes();
        if hardware.ram.total_bytes != 0 && bytes > hardware.ram.total_bytes {
            return Err(resource_error(
                "storage workload logical bytes exceed declared RAM total",
            ));
        }
        if let Some(max_bytes) = hardware.ram.section_budget_bytes(role) {
            if bytes > max_bytes {
                return Err(resource_error(
                    "storage workload logical bytes exceed RAM section budget",
                ));
            }
        }
    }

    Ok(())
}

const fn io_path_for_tier(tier: StorageTier) -> IoPathClass {
    match tier {
        StorageTier::Ram => IoPathClass::CpuRam,
        StorageTier::HotStore => IoPathClass::HotPathNvmeSsd,
        StorageTier::ColdStore => IoPathClass::ColdPathHdd,
    }
}

fn resource_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Resource, message)
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AllocationId, ExtentId, Lsn, ObjectId, PageId, PageSize, SegmentHeader, SegmentId,
        SegmentTrailer,
    };
    use andromeda_core::{GpuExecutionPolicy, GpuProfile, RamProfile, RamSectionBudget};

    fn descriptor(state: SegmentState) -> SegmentDescriptor {
        let header = SegmentHeader {
            magic: SegmentHeader::MAGIC,
            format_version: SegmentHeader::FORMAT_VERSION_V0,
            segment_id: SegmentId::new(90),
            object_id: ObjectId::new(91),
            allocation_id: AllocationId::new(92),
            first_page_id: PageId::new(1_000),
            page_count: 8,
            min_page_lsn: Lsn::new(93),
            max_page_lsn: Lsn::new(94),
            header_crc: 95,
        };

        SegmentDescriptor {
            segment_id: header.segment_id,
            object_id: header.object_id,
            allocation_id: header.allocation_id,
            first_extent_id: ExtentId::new(96),
            extent_count: 1,
            first_page_id: header.first_page_id,
            page_count: header.page_count,
            page_size: PageSize::KiB16,
            min_page_lsn: header.min_page_lsn,
            max_page_lsn: header.max_page_lsn,
            snapshot_id: match state {
                SegmentState::BuildingHotSnapshot => None,
                SegmentState::Sealed | SegmentState::PublishedCold => Some(97),
            },
            state,
            header,
            trailer: SegmentTrailer {
                segment_payload_crc64: 98,
                segment_hash: [99; 32],
                trailer_crc: 100,
            },
        }
    }

    fn hot_budget() -> IoPathBudget {
        IoPathBudget::new(
            IoPathClass::HotPathNvmeSsd,
            crate::IoLatencyBudget::new(1_000, 1_000, 2_000),
            crate::IoThroughputBudget::new(128 * 1024 * 1024, 128 * 1024 * 1024),
        )
    }

    fn cold_budget() -> IoPathBudget {
        IoPathBudget::new(
            IoPathClass::ColdPathHdd,
            crate::IoLatencyBudget::new(5_000_000, 5_000_000, 5_000_000),
            crate::IoThroughputBudget::new(128 * 1024 * 1024, 128 * 1024 * 1024),
        )
    }

    fn ram_budget() -> IoPathBudget {
        IoPathBudget::new(
            IoPathClass::CpuRam,
            crate::IoLatencyBudget::new(1_000, 1_000, 1_000),
            crate::IoThroughputBudget::new(128 * 1024 * 1024, 128 * 1024 * 1024),
        )
    }

    fn hardware_with_gpu() -> HardwareProfile {
        HardwareProfile {
            ram: RamProfile::new(
                256 * 1024 * 1024,
                vec![
                    RamSectionBudget::new(RamSectionRole::Cache, 64 * 1024 * 1024),
                    RamSectionBudget::new(RamSectionRole::Io, 64 * 1024 * 1024),
                    RamSectionBudget::new(RamSectionRole::Execution, 64 * 1024 * 1024),
                ],
            ),
            gpu: GpuProfile {
                available: true,
                execution_policy: GpuExecutionPolicy::OffCriticalPathOnly,
            },
            ..HardwareProfile::conservative()
        }
    }

    #[test]
    fn append_uses_hotstore_and_rejects_cold_temperature() {
        let decision = PlacementDecision::append(DataTemperature::Hot).unwrap();

        assert_eq!(decision.target_tier, StorageTier::HotStore);
        assert_eq!(decision.pipeline_stage, PipelineStage::AppendHotStore);
        assert!(decision.mutation_allowed);
        assert!(decision.validate().is_ok());

        assert_eq!(
            PlacementDecision::append(DataTemperature::Cold)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn reads_follow_temperature_fallback_rules() {
        let ram = PlacementDecision::read(DataTemperature::RamWorkingSet);
        assert_eq!(ram.target_tier, StorageTier::Ram);
        assert!(ram.read_fallback.allows_tier(StorageTier::ColdStore));
        assert!(ram.validate().is_ok());

        let cold = PlacementDecision::read(DataTemperature::Cold);
        assert_eq!(cold.target_tier, StorageTier::ColdStore);
        assert!(cold.read_fallback.allows_tier(StorageTier::ColdStore));
        assert!(!cold.read_fallback.allows_tier(StorageTier::HotStore));
        assert!(cold.validate().is_ok());
    }

    #[test]
    fn hot_to_cold_pipeline_requires_sealed_snapshot_segment() {
        let building = descriptor(SegmentState::BuildingHotSnapshot);
        let seal = PlacementDecision::seal_hot_segment(&building).unwrap();
        assert_eq!(seal.pipeline_stage, PipelineStage::SealHotStoreSegment);
        assert_eq!(seal.target_tier, StorageTier::HotStore);
        assert!(seal.validate().is_ok());

        let sealed = descriptor(SegmentState::Sealed);
        let publish = PlacementDecision::publish_cold_segment(&sealed).unwrap();
        assert_eq!(publish.pipeline_stage, PipelineStage::PublishColdStore);
        assert_eq!(publish.target_tier, StorageTier::ColdStore);
        assert!(!publish.mutation_allowed);
        assert!(publish.validate().is_ok());

        let published = descriptor(SegmentState::PublishedCold);
        assert_eq!(
            PlacementDecision::mutation(
                &published,
                SegmentMutation::AppendExtent,
                DataTemperature::Hot,
            )
            .unwrap_err()
            .kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn core_io_policy_maps_ram_hot_and_cold_workloads_to_storage_tiers() {
        let policy =
            CoreIoPlacementPolicy::new(hardware_with_gpu(), HotColdIoThresholds::conservative());

        let ram = policy
            .plan(CoreIoPlacementRequest::new(
                StorageWorkloadClass::RamWorkingSet,
                StorageIoBudgetScope::Page(PageSize::KiB16),
                ram_budget(),
                false,
            ))
            .unwrap();
        assert_eq!(ram.placement.target_tier, StorageTier::Ram);
        assert_eq!(ram.path_budget.path_class, IoPathClass::CpuRam);

        let hot = policy
            .plan(CoreIoPlacementRequest::new(
                StorageWorkloadClass::HotAppend,
                StorageIoBudgetScope::Page(PageSize::KiB16),
                hot_budget(),
                false,
            ))
            .unwrap();
        assert_eq!(hot.placement.target_tier, StorageTier::HotStore);
        assert_eq!(hot.io_use_class, IoUseClass::OnlineHotPath);

        let cold_read = policy
            .plan(CoreIoPlacementRequest::new(
                StorageWorkloadClass::ColdRead,
                StorageIoBudgetScope::Page(PageSize::KiB16),
                cold_budget(),
                false,
            ))
            .unwrap();
        assert_eq!(cold_read.placement.target_tier, StorageTier::ColdStore);
        assert_eq!(cold_read.io_use_class, IoUseClass::ColdSegmentPath);
        assert!(!cold_read.placement.mutation_allowed);
    }

    #[test]
    fn core_io_policy_rejects_gpu_for_commit_wal_rollback_and_recovery() {
        let policy =
            CoreIoPlacementPolicy::new(hardware_with_gpu(), HotColdIoThresholds::conservative());

        for workload in [
            StorageWorkloadClass::Commit,
            StorageWorkloadClass::WalAppend,
            StorageWorkloadClass::Rollback,
            StorageWorkloadClass::Recovery,
        ] {
            let error = policy
                .plan(CoreIoPlacementRequest::new(
                    workload,
                    StorageIoBudgetScope::Page(PageSize::KiB16),
                    hot_budget(),
                    true,
                ))
                .unwrap_err();
            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        }
    }

    #[test]
    fn core_io_policy_rejects_cold_hdd_for_commit_critical_hot_path() {
        let policy =
            CoreIoPlacementPolicy::new(hardware_with_gpu(), HotColdIoThresholds::conservative());

        let error = policy
            .plan(CoreIoPlacementRequest::new(
                StorageWorkloadClass::Commit,
                StorageIoBudgetScope::Page(PageSize::KiB16),
                cold_budget(),
                false,
            ))
            .unwrap_err();
        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    }

    #[test]
    fn core_io_policy_validates_ram_and_segment_io_budgets() {
        let policy =
            CoreIoPlacementPolicy::new(hardware_with_gpu(), HotColdIoThresholds::conservative());

        let over_ram = policy
            .plan(CoreIoPlacementRequest::new(
                StorageWorkloadClass::RamWorkingSet,
                StorageIoBudgetScope::Segment {
                    bytes: 128 * 1024 * 1024,
                },
                ram_budget(),
                false,
            ))
            .unwrap_err();
        assert_eq!(over_ram.kind(), AndromedaErrorKind::Resource);

        let cold_publication = policy
            .plan(CoreIoPlacementRequest::new(
                StorageWorkloadClass::ColdPublication,
                StorageIoBudgetScope::Segment {
                    bytes: 128 * 1024 * 1024,
                },
                cold_budget(),
                false,
            ))
            .unwrap();
        assert_eq!(
            cold_publication.placement.pipeline_stage,
            PipelineStage::PublishColdStore
        );
        assert_eq!(
            cold_publication.placement.target_tier,
            StorageTier::ColdStore
        );
    }
}
