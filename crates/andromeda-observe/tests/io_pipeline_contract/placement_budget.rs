use crate::support::*;

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
        },
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
        },
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
