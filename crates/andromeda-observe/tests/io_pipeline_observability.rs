use andromeda_core::{
    ContractHash, CpuProfile, GpuExecutionPolicy, GpuProfile, HardwareArchitecture,
    HardwareProfile, PipelineClass, RamProfile, RamSectionBudget, RamSectionRole, ResourceBudget,
    SessionId, TransactionId,
};
use andromeda_observe::{
    CriticalDecisionKind, EventCorrelation, EventEnvelope, EventId, GpuPolicyDecisionTrace,
    IoBudgetDecisionTrace, IoPipelineStage, IoPlacementDecisionTrace, IoStorageTier, TraceEvent,
    TraceId,
};
use andromeda_storage::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, CoreIoPlacementRequest, HotColdIoThresholds,
    IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget, PageSize, PipelineStage,
    PlacementDecision, StorageIoBudgetScope, StorageTier, StorageWorkloadClass,
};

#[test]
fn io_pipeline_decisions_preserve_tier_pipeline_budget_and_reason_evidence() {
    let placement = EventEnvelope::new(
        EventId::new(1),
        EventCorrelation::empty(),
        TraceEvent::IoPlacementDecision(
            IoPlacementDecisionTrace::accepted(
                TraceId::new(101),
                PipelineClass::ForegroundExecution,
                IoPipelineStage::Hot,
                IoStorageTier::Hot,
                "hot tier selected for foreground IO placement policy",
            )
            .expect("placement helper requires explicit reason evidence"),
        ),
    )
    .expect("IO placement decision has typed tier, pipeline, stage, and reason evidence");

    assert_eq!(
        placement.event.kind(),
        CriticalDecisionKind::IoPlacementDecision
    );
    match &placement.event {
        TraceEvent::IoPlacementDecision(trace) => {
            assert_eq!(trace.pipeline, PipelineClass::ForegroundExecution);
            assert_eq!(trace.stage, IoPipelineStage::Hot);
            assert_eq!(trace.selected_tier, IoStorageTier::Hot);
            assert!(trace.accepted);
            assert!(trace.reason.contains("hot tier"));
        }
        _ => panic!("expected IO placement decision trace"),
    }

    let budget = EventEnvelope::new(
        EventId::new(2),
        EventCorrelation::empty(),
        TraceEvent::IoBudgetDecision(
            IoBudgetDecisionTrace::from_budget_request(
                TraceId::new(102),
                PipelineClass::BackgroundMaintenance,
                IoPipelineStage::Ram,
                ResourceBudget::new(1024, 2048, 4),
                512,
                1024,
                2,
                "RAM stage IO budget request is within declared limits",
            )
            .expect("budget helper requires explicit reason evidence"),
        ),
    )
    .expect("IO budget decision has explicit budget and reason evidence");

    assert_eq!(
        budget.event.kind(),
        CriticalDecisionKind::IoBudgetValidation
    );
    match &budget.event {
        TraceEvent::IoBudgetDecision(trace) => {
            assert_eq!(trace.pipeline, PipelineClass::BackgroundMaintenance);
            assert_eq!(trace.stage, IoPipelineStage::Ram);
            assert_eq!(trace.budget.max_memory_bytes, 1024);
            assert_eq!(trace.budget.max_temp_bytes, 2048);
            assert_eq!(trace.budget.max_streams, 4);
            assert!(trace.accepted);
            assert!(trace.reason.contains("within declared limits"));
        }
        _ => panic!("expected IO budget decision trace"),
    }

    let cold_rejection = EventEnvelope::new(
        EventId::new(3),
        EventCorrelation::empty(),
        TraceEvent::IoBudgetDecision(
            IoBudgetDecisionTrace::from_budget_request(
                TraceId::new(103),
                PipelineClass::BackgroundMaintenance,
                IoPipelineStage::Cold,
                ResourceBudget::new(1024, 2048, 4),
                2048,
                1024,
                2,
                "cold stage IO budget rejected because memory request exceeds limit",
            )
            .expect("budget helper requires explicit reason evidence"),
        ),
    )
    .expect("IO budget rejection records requested values, budget limits, stage, and reason");

    assert_eq!(
        cold_rejection.event.kind(),
        CriticalDecisionKind::IoBudgetValidation
    );
}

#[test]
fn gpu_policy_decisions_record_acceptance_and_rejection_without_hardware_claims() {
    let rejected = EventEnvelope::new(
        EventId::new(10),
        EventCorrelation::empty(),
        TraceEvent::GpuPolicyDecision(
            GpuPolicyDecisionTrace::from_policy(
                TraceId::new(201),
                PipelineClass::Commit,
                GpuExecutionPolicy::OffCriticalPathOnly,
                true,
                "GPU policy rejected commit pipeline because it is critical path",
            )
            .expect("GPU helper requires explicit reason evidence"),
        ),
    )
    .expect("GPU rejection records policy decision without hardware measurements");

    assert_eq!(
        rejected.event.kind(),
        CriticalDecisionKind::GpuPolicyDecision
    );
    match &rejected.event {
        TraceEvent::GpuPolicyDecision(trace) => {
            assert_eq!(trace.pipeline, PipelineClass::Commit);
            assert_eq!(trace.policy, GpuExecutionPolicy::OffCriticalPathOnly);
            assert!(trace.gpu_declared_available);
            assert!(!trace.accepted);
            assert!(trace.reason.contains("critical path"));
        }
        _ => panic!("expected GPU policy decision trace"),
    }

    let accepted = EventEnvelope::new(
        EventId::new(11),
        EventCorrelation::empty(),
        TraceEvent::GpuPolicyDecision(
            GpuPolicyDecisionTrace::from_policy(
                TraceId::new(202),
                PipelineClass::BatchAnalytics,
                GpuExecutionPolicy::BatchAnalyticsOnly,
                true,
                "GPU policy accepted batch analytics pipeline",
            )
            .expect("GPU helper requires explicit reason evidence"),
        ),
    )
    .expect("GPU acceptance records policy decision for an allowed off-critical pipeline");

    assert_eq!(
        accepted.event.kind(),
        CriticalDecisionKind::GpuPolicyDecision
    );
}

#[test]
fn io_and_gpu_observability_rejects_default_or_policy_inconsistent_evidence() {
    let missing_budget = EventEnvelope::new(
        EventId::new(20),
        EventCorrelation::empty(),
        TraceEvent::IoBudgetDecision(IoBudgetDecisionTrace {
            trace_id: TraceId::new(301),
            pipeline: PipelineClass::ForegroundExecution,
            stage: IoPipelineStage::Ram,
            budget: ResourceBudget::new(0, 0, 0),
            requested_memory_bytes: 0,
            requested_temp_bytes: 0,
            requested_streams: 0,
            accepted: true,
            reason: "budget defaults must not masquerade as successful evidence".to_string(),
        }),
    )
    .unwrap_err();

    assert!(missing_budget.message().contains("budget limit evidence"));

    let critical_path_gpu_acceptance = EventEnvelope::new(
        EventId::new(21),
        EventCorrelation::empty(),
        TraceEvent::GpuPolicyDecision(GpuPolicyDecisionTrace {
            trace_id: TraceId::new(302),
            pipeline: PipelineClass::WalAppend,
            policy: GpuExecutionPolicy::OffCriticalPathOnly,
            gpu_declared_available: true,
            accepted: true,
            reason: "GPU policy cannot accept WAL append critical path".to_string(),
        }),
    )
    .unwrap_err();

    assert!(critical_path_gpu_acceptance
        .message()
        .contains("GPU policy decision outcome"));
}

#[test]
fn io_and_gpu_helpers_reject_empty_reasons_and_preserve_outcome_mapping() {
    assert!(
        IoPlacementDecisionTrace::accepted(
            TraceId::new(401),
            PipelineClass::ForegroundExecution,
            IoPipelineStage::Hot,
            IoStorageTier::Hot,
            "   ",
        )
        .is_err(),
        "placement helper must preserve explicit reason validation"
    );

    let budget_rejection = IoBudgetDecisionTrace::from_budget_request(
        TraceId::new(402),
        PipelineClass::BackgroundMaintenance,
        IoPipelineStage::Cold,
        ResourceBudget::new(1024, 2048, 4),
        1025,
        0,
        1,
        "budget helper maps over-limit memory requests to rejection",
    )
    .expect("non-empty reason should construct budget evidence");
    assert!(!budget_rejection.accepted);
    assert!(budget_rejection.outcome_matches_budget());

    let gpu_rejection = GpuPolicyDecisionTrace::from_profile(
        TraceId::new(403),
        PipelineClass::Commit,
        GpuProfile::off_critical_path(),
        "GPU helper maps critical-path policy rejection",
    )
    .expect("non-empty reason should construct GPU evidence");
    assert!(!gpu_rejection.accepted);
    assert!(gpu_rejection.outcome_matches_policy());
}

#[test]
fn evidence_factories_reject_empty_reasons_and_manual_outcome_drift() {
    let rejected_placement = IoPlacementDecisionTrace::rejected(
        TraceId::new(451),
        PipelineClass::ForegroundExecution,
        IoPipelineStage::Hot,
        IoStorageTier::Hot,
        "placement factory records explicit rejection reason",
    )
    .expect("placement rejection factory requires explicit reason evidence");
    assert!(!rejected_placement.accepted);
    EventEnvelope::new(
        EventId::new(451),
        EventCorrelation::empty(),
        TraceEvent::IoPlacementDecision(rejected_placement),
    )
    .expect("factory-built placement rejection should be valid observability evidence");

    let budget_rejection = IoBudgetDecisionTrace::from_budget_request(
        TraceId::new(452),
        PipelineClass::BackgroundMaintenance,
        IoPipelineStage::Cold,
        ResourceBudget::new(1024, 2048, 1),
        1025,
        0,
        1,
        "budget factory records rejection when requested memory exceeds declared limit",
    )
    .expect("budget factory requires explicit reason evidence");
    assert!(!budget_rejection.accepted);
    assert!(budget_rejection.outcome_matches_budget());
    EventEnvelope::new(
        EventId::new(452),
        EventCorrelation::empty(),
        TraceEvent::IoBudgetDecision(budget_rejection),
    )
    .expect("factory-built budget rejection should preserve coherent outcome evidence");

    let manual_drift = EventEnvelope::new(
        EventId::new(453),
        EventCorrelation::empty(),
        TraceEvent::IoBudgetDecision(IoBudgetDecisionTrace {
            trace_id: TraceId::new(453),
            pipeline: PipelineClass::BackgroundMaintenance,
            stage: IoPipelineStage::Cold,
            budget: ResourceBudget::new(1024, 2048, 1),
            requested_memory_bytes: 1025,
            requested_temp_bytes: 0,
            requested_streams: 1,
            accepted: true,
            reason: "manual evidence drift must not override budget outcome".to_string(),
        }),
    )
    .unwrap_err();
    assert!(manual_drift.message().contains("outcome must match"));

    assert!(
        IoBudgetDecisionTrace::from_budget_request(
            TraceId::new(454),
            PipelineClass::BackgroundMaintenance,
            IoPipelineStage::Cold,
            ResourceBudget::new(1024, 2048, 1),
            1,
            0,
            1,
            "   ",
        )
        .is_err(),
        "budget factory must not create evidence without an explicit reason"
    );
}

#[test]
fn core_io_workflow_correlates_placement_budget_and_gpu_policy_traces() {
    let policy = workflow_policy();
    let correlation = workflow_correlation();

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
        }
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

fn workflow_correlation() -> EventCorrelation {
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
        }
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
        }
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
        }
        PipelineStage::AppendHotStore | PipelineStage::SealHotStoreSegment
            if placement.target_tier == StorageTier::HotStore =>
        {
            IoPipelineStage::Hot
        }
        PipelineStage::PublishColdStore | PipelineStage::ServeRead
            if placement.target_tier == StorageTier::ColdStore =>
        {
            IoPipelineStage::Cold
        }
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
