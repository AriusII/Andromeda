use crate::support::*;

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

    assert!(
        critical_path_gpu_acceptance
            .message()
            .contains("GPU policy decision outcome")
    );
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
