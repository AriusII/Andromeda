use andromeda_decision_trace::{CriticalDecisionKind, CriticalDecisionTrace};
use andromeda_observability::TraceId;

#[test]
fn critical_decision_trace_keeps_observe_shape_runtime_free() {
    let trace = CriticalDecisionTrace {
        trace_id: TraceId::new(42),
        decision: CriticalDecisionKind::PlanSelection,
        reason: "bounded plan selected".to_string(),
    };

    assert!(trace.has_explanation());
}
