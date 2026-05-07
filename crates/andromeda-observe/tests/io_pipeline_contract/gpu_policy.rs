use crate::support::*;

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
