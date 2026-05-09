pub(crate) use andromeda_hardware::{
    CpuProfile, GpuExecutionPolicy, GpuProfile, HardwareArchitecture, HardwareProfile,
    PipelineClass, RamProfile, RamSectionBudget, RamSectionRole, ResourceBudget,
};
pub(crate) use andromeda_observe::{
    CriticalDecisionKind, EventCorrelation, EventEnvelope, EventId, GpuPolicyDecisionTrace,
    IoBudgetDecisionTrace, IoPipelineStage, IoPlacementDecisionTrace, IoStorageTier, TraceEvent,
    TraceId,
};
pub(crate) use andromeda_storage::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, CoreIoPlacementRequest, HotColdIoThresholds,
    IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget, PageSize, PipelineStage,
    PlacementDecision, StorageIoBudgetScope, StorageTier, StorageWorkloadClass,
};
pub(crate) use andromeda_types::{ContractHash, SessionId, TransactionId};

pub(crate) fn workflow_policy() -> CoreIoPlacementPolicy {
    let cpu = CpuProfile::conservative();
    CoreIoPlacementPolicy::new(
        HardwareProfile {
            architecture: HardwareArchitecture::Unknown,
            has_simd: cpu.supports_simd(),
            has_direct_io: false,
            cpu,
            ram: RamProfile::new(
                256 * 1024 * 1024,
                vec![
                    RamSectionBudget::new(RamSectionRole::Io, 64 * 1024 * 1024),
                    RamSectionBudget::new(RamSectionRole::Cache, 64 * 1024 * 1024),
                    RamSectionBudget::new(RamSectionRole::Execution, 64 * 1024 * 1024),
                ],
            ),
            gpu: GpuProfile::off_critical_path(),
        },
        HotColdIoThresholds::conservative(),
    )
}

pub(crate) fn workflow_correlation() -> EventCorrelation {
    EventCorrelation {
        request_id: Some(42.into()),
        session_id: Some(SessionId::new(7)),
        contract_hash: Some(ContractHash::test_vector(5)),
        catalog_version: Some(3.into()),
        catalog_object_id: None,
        transaction_id: Some(TransactionId::new(9)),
        durable_lsn: Some(10),
        protocol: None,
    }
}

pub(crate) fn trace_storage_decision(
    first_event_id: EventId,
    trace_id: TraceId,
    correlation: EventCorrelation,
    decision: &CoreIoPlacementDecision,
    scope: StorageIoBudgetScope,
    scenario: &'static str,
) -> [EventEnvelope; 2] {
    let placement = EventEnvelope::new(
        first_event_id,
        correlation,
        TraceEvent::IoPlacementDecision(
            IoPlacementDecisionTrace::accepted(
                trace_id,
                decision.pipeline_class,
                stage_from_placement(decision.placement),
                tier_from_storage(decision.placement.target_tier),
                format!(
                    "{scenario} placement accepted because {}",
                    decision.placement.reason
                ),
            )
            .expect("placement helper requires explicit reason evidence"),
        ),
    )
    .expect("placement trace must include tier, stage, pipeline, correlation, and reason");

    let (budget, requested_memory_bytes, requested_temp_bytes, requested_streams) =
        resource_budget_for(decision.placement, scope);
    let budget = EventEnvelope::new(
        EventId::new(first_event_id.get() + 1),
        correlation,
        TraceEvent::IoBudgetDecision(
            IoBudgetDecisionTrace::from_budget_request(
                trace_id,
                decision.pipeline_class,
                stage_from_placement(decision.placement),
                budget,
                requested_memory_bytes,
                requested_temp_bytes,
                requested_streams,
                format!(
                    "{scenario} budget accepted because requested IO resources are within declared workflow limits"
                ),
            )
                .expect("budget helper requires explicit reason evidence"),
        ),
    )
        .expect("budget trace must include requested resources, limits, and reason");

    [placement, budget]
}

pub(crate) fn resource_budget_for(
    placement: PlacementDecision,
    scope: StorageIoBudgetScope,
) -> (ResourceBudget, u64, u64, u32) {
    let logical_bytes = scope.logical_bytes();
    match placement.target_tier {
        StorageTier::Ram | StorageTier::HotStore => (
            ResourceBudget::new(logical_bytes * 2, 64 * 1024 * 1024, 4),
            logical_bytes,
            0,
            1,
        ),
        StorageTier::ColdStore => (
            ResourceBudget::new(16 * 1024 * 1024, logical_bytes * 2, 4),
            0,
            logical_bytes,
            1,
        ),
    }
}

pub(crate) fn assert_correlated_decision_pair(
    events: &[EventEnvelope; 2],
    correlation: EventCorrelation,
    pipeline: PipelineClass,
    tier: IoStorageTier,
    stage: IoPipelineStage,
) {
    assert_eq!(events[0].correlation, correlation);
    assert_eq!(events[1].correlation, correlation);
    assert_eq!(events[0].trace_id, events[1].trace_id);
    assert_eq!(
        events[0].event.kind(),
        CriticalDecisionKind::IoPlacementDecision
    );
    assert_eq!(
        events[1].event.kind(),
        CriticalDecisionKind::IoBudgetValidation
    );

    match &events[0].event {
        TraceEvent::IoPlacementDecision(trace) => {
            assert_eq!(trace.pipeline, pipeline);
            assert_eq!(trace.stage, stage);
            assert_eq!(trace.selected_tier, tier);
            assert!(trace.accepted);
            assert!(trace.has_reason());
            assert!(trace.reason.contains("because"));
        },
        _ => panic!("expected placement trace"),
    }

    match &events[1].event {
        TraceEvent::IoBudgetDecision(trace) => {
            assert_eq!(trace.pipeline, pipeline);
            assert_eq!(trace.stage, stage);
            assert!(trace.accepted);
            assert!(trace.has_budget_evidence());
            assert!(trace.outcome_matches_budget());
            assert!(trace.reason.contains("because"));
        },
        _ => panic!("expected budget trace"),
    }
}

pub(crate) fn tier_from_storage(tier: StorageTier) -> IoStorageTier {
    match tier {
        StorageTier::Ram => IoStorageTier::Ram,
        StorageTier::HotStore => IoStorageTier::Hot,
        StorageTier::ColdStore => IoStorageTier::Cold,
    }
}

pub(crate) fn stage_from_placement(placement: PlacementDecision) -> IoPipelineStage {
    match placement.pipeline_stage {
        PipelineStage::BufferInRam | PipelineStage::ServeRead
            if placement.target_tier == StorageTier::Ram =>
        {
            IoPipelineStage::Ram
        },
        PipelineStage::AppendHotStore | PipelineStage::SealHotStoreSegment
            if placement.target_tier == StorageTier::HotStore =>
        {
            IoPipelineStage::Hot
        },
        PipelineStage::PublishColdStore | PipelineStage::ServeRead
            if placement.target_tier == StorageTier::ColdStore =>
        {
            IoPipelineStage::Cold
        },
        _ => tier_stage_fallback(placement.target_tier),
    }
}

pub(crate) fn tier_stage_fallback(tier: StorageTier) -> IoPipelineStage {
    match tier {
        StorageTier::Ram => IoPipelineStage::Ram,
        StorageTier::HotStore => IoPipelineStage::Hot,
        StorageTier::ColdStore => IoPipelineStage::Cold,
    }
}

pub(crate) fn ram_path_budget() -> IoPathBudget {
    IoPathBudget::new(
        IoPathClass::CpuRam,
        IoLatencyBudget::new(1_000, 1_000, 1_000),
        IoThroughputBudget::new(64 * 1024 * 1024, 64 * 1024 * 1024),
    )
}

pub(crate) fn hot_path_budget() -> IoPathBudget {
    IoPathBudget::new(
        IoPathClass::HotPathNvmeSsd,
        IoLatencyBudget::new(1_000, 1_000, 2_000),
        IoThroughputBudget::new(64 * 1024 * 1024, 64 * 1024 * 1024),
    )
}

pub(crate) fn cold_path_budget() -> IoPathBudget {
    IoPathBudget::new(
        IoPathClass::ColdPathHdd,
        IoLatencyBudget::new(5_000_000, 5_000_000, 5_000_000),
        IoThroughputBudget::new(64 * 1024 * 1024, 64 * 1024 * 1024),
    )
}
