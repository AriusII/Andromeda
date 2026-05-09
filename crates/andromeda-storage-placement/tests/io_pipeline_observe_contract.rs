use andromeda_hardware::{
    CpuProfile, GpuProfile, HardwareArchitecture, HardwareProfile, PipelineClass, RamProfile,
    RamSectionBudget, RamSectionRole, ResourceBudget,
};
use andromeda_observe::{
    CriticalDecisionKind, EventCorrelation, EventEnvelope, EventId, GpuPolicyDecisionTrace,
    IoBudgetDecisionTrace, IoPipelineStage, IoPlacementDecisionTrace, IoStorageTier, TraceEvent,
    TraceId,
};
use andromeda_storage_page::PageSize;
use andromeda_storage_placement::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, CoreIoPlacementRequest, HotColdIoThresholds,
    IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget, PipelineStage,
    PlacementDecision, StorageIoBudgetScope, StorageTier, StorageWorkloadClass,
};

fn workflow_policy() -> CoreIoPlacementPolicy {
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

fn trace_storage_decision(
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

fn resource_budget_for(
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

fn assert_correlated_decision_pair(
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

fn tier_from_storage(tier: StorageTier) -> IoStorageTier {
    match tier {
        StorageTier::Ram => IoStorageTier::Ram,
        StorageTier::HotStore => IoStorageTier::Hot,
        StorageTier::ColdStore => IoStorageTier::Cold,
    }
}

fn stage_from_placement(placement: PlacementDecision) -> IoPipelineStage {
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

fn tier_stage_fallback(tier: StorageTier) -> IoPipelineStage {
    match tier {
        StorageTier::Ram => IoPipelineStage::Ram,
        StorageTier::HotStore => IoPipelineStage::Hot,
        StorageTier::ColdStore => IoPipelineStage::Cold,
    }
}

fn ram_path_budget() -> IoPathBudget {
    IoPathBudget::new(
        IoPathClass::CpuRam,
        IoLatencyBudget::new(1_000, 1_000, 1_000),
        IoThroughputBudget::new(64 * 1024 * 1024, 64 * 1024 * 1024),
    )
}

fn hot_path_budget() -> IoPathBudget {
    IoPathBudget::new(
        IoPathClass::HotPathNvmeSsd,
        IoLatencyBudget::new(1_000, 1_000, 2_000),
        IoThroughputBudget::new(64 * 1024 * 1024, 64 * 1024 * 1024),
    )
}

fn cold_path_budget() -> IoPathBudget {
    IoPathBudget::new(
        IoPathClass::ColdPathHdd,
        IoLatencyBudget::new(5_000_000, 5_000_000, 5_000_000),
        IoThroughputBudget::new(64 * 1024 * 1024, 64 * 1024 * 1024),
    )
}

#[test]
fn core_io_workflow_correlates_placement_budget_and_gpu_policy_traces() {
    let policy = workflow_policy();
    let correlation = EventCorrelation::empty();

    let hot_commit = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::Commit,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            hot_path_budget(),
            false,
        ))
        .expect("hot commit-safe workflow must place commit IO without GPU on the critical path");
    assert_eq!(hot_commit.pipeline_class, PipelineClass::Commit);
    assert_eq!(hot_commit.placement.target_tier, StorageTier::HotStore);

    let hot_commit_events = trace_storage_decision(
        EventId::new(100),
        TraceId::new(1_000),
        correlation,
        &hot_commit,
        StorageIoBudgetScope::Page(PageSize::KiB16),
        "hot commit-safe",
    );
    assert_correlated_decision_pair(
        &hot_commit_events,
        correlation,
        PipelineClass::Commit,
        IoStorageTier::Hot,
        IoPipelineStage::Hot,
    );

    let critical_gpu_error = policy
        .hardware
        .validate_gpu_pipeline(PipelineClass::Commit)
        .expect_err("GPU policy must reject the commit critical path");
    let critical_path_gpu_rejection = EventEnvelope::new(
        EventId::new(102),
        correlation,
        TraceEvent::GpuPolicyDecision(
            GpuPolicyDecisionTrace::from_profile(
                TraceId::new(1_000),
                PipelineClass::Commit,
                policy.hardware.gpu,
                format!(
                    "GPU policy rejected hot commit-safe critical path because {}",
                    critical_gpu_error.message()
                ),
            )
            .expect("GPU helper requires explicit reason evidence"),
        ),
    )
    .expect("GPU rejection trace carries explicit critical-path reason evidence");

    match &critical_path_gpu_rejection.event {
        TraceEvent::GpuPolicyDecision(trace) => {
            assert!(!trace.accepted);
            assert!(trace.reason.contains("critical path"));
            assert!(trace.reason.contains("commit"));
        },
        _ => panic!("expected GPU policy decision trace"),
    }

    let ram_working_set = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::RamWorkingSet,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            ram_path_budget(),
            false,
        ))
        .expect("analytics preparation workflow must preserve RAM working-set placement");
    let ram_events = trace_storage_decision(
        EventId::new(110),
        TraceId::new(1_100),
        correlation,
        &ram_working_set,
        StorageIoBudgetScope::Page(PageSize::KiB16),
        "analytics RAM working-set",
    );
    assert_correlated_decision_pair(
        &ram_events,
        correlation,
        PipelineClass::ForegroundExecution,
        IoStorageTier::Ram,
        IoPipelineStage::Ram,
    );

    let cold_publication = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::ColdPublication,
            StorageIoBudgetScope::Segment {
                bytes: 128 * 1024 * 1024,
            },
            cold_path_budget(),
            false,
        ))
        .expect("analytics off-critical-path workflow must allow immutable ColdStore publication");
    let cold_events = trace_storage_decision(
        EventId::new(120),
        TraceId::new(1_200),
        correlation,
        &cold_publication,
        StorageIoBudgetScope::Segment {
            bytes: 128 * 1024 * 1024,
        },
        "analytics off-critical-path ColdStore publication",
    );
    assert_correlated_decision_pair(
        &cold_events,
        correlation,
        PipelineClass::BackgroundMaintenance,
        IoStorageTier::Cold,
        IoPipelineStage::Cold,
    );

    policy
        .hardware
        .validate_gpu_pipeline(PipelineClass::BatchAnalytics)
        .expect("off-critical-path GPU policy permits batch analytics pipeline");
    let analytics_gpu_acceptance = EventEnvelope::new(
        EventId::new(122),
        correlation,
        TraceEvent::GpuPolicyDecision(
            GpuPolicyDecisionTrace::from_profile(
                TraceId::new(1_200),
                PipelineClass::BatchAnalytics,
                policy.hardware.gpu,
                "GPU policy accepted analytics off-critical-path workflow because batch analytics is not commit critical",
            )
                .expect("GPU helper requires explicit reason evidence"),
        ),
    )
        .expect("GPU acceptance trace carries explicit off-critical-path reason evidence");

    assert_eq!(
        analytics_gpu_acceptance.event.kind(),
        CriticalDecisionKind::GpuPolicyDecision
    );
    assert_eq!(critical_path_gpu_rejection.correlation, correlation);
    assert_eq!(analytics_gpu_acceptance.correlation, correlation);
}
