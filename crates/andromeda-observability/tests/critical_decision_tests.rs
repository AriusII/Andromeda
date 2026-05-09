use andromeda_observability::{CriticalDecisionKind, CriticalDecisionTrace, TraceId};

#[test]
fn critical_decision_trace_keeps_observability_shape_runtime_free() {
    let trace = CriticalDecisionTrace {
        trace_id: TraceId::new(42),
        decision: CriticalDecisionKind::PlanSelection,
        reason: "bounded plan selected".to_string(),
    };

    assert!(trace.has_explanation());
}
