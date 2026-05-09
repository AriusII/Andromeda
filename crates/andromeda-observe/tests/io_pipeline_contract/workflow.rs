use crate::support::*;

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
