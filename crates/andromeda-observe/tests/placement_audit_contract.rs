use andromeda_observability::{CriticalDecisionKind, EventCorrelation, EventId, TraceId};
use andromeda_observe::{
    EventEnvelope, EventSink, InMemoryEventSink, PlacementAuditEvent, PlacementAuditTransition,
    TraceEvent,
};

#[test]
fn placement_audit_event_roundtrip_covers_all_transitions() {
    let events = [
        PlacementAuditEvent::placement_decision_made(
            TraceId::new(10),
            "placement policy selected storage path",
        )
        .unwrap(),
        PlacementAuditEvent::segment_sealed(TraceId::new(11), 100, "segment sealed on hot tier")
            .unwrap(),
        PlacementAuditEvent::segment_published_cold(
            TraceId::new(12),
            100,
            "segment published to cold tier",
        )
        .unwrap(),
        PlacementAuditEvent::extent_reclaimed(TraceId::new(13), 501, "extent reclaimed").unwrap(),
        PlacementAuditEvent::cold_mutation_rejected(
            TraceId::new(14),
            100,
            "cold mutation rejected after publication",
        )
        .unwrap(),
    ];

    let mut sink = InMemoryEventSink::new();

    for (idx, event) in events.into_iter().enumerate() {
        let envelope = EventEnvelope::new(
            EventId::new((idx + 1) as u128),
            EventCorrelation::empty(),
            TraceEvent::PlacementAudit(event),
        )
        .expect("placement audit envelope must validate");
        sink.emit(envelope).expect("placement audit sink append");
    }

    let recorded = sink.events();
    assert_eq!(recorded.len(), 5);
    assert!(
        recorded
            .iter()
            .all(|event| event.event.kind() == CriticalDecisionKind::PlacementAudit)
    );

    let transitions: Vec<_> = recorded
        .iter()
        .map(|event| match &event.event {
            TraceEvent::PlacementAudit(trace) => trace.transition,
            other => panic!("expected placement audit event, got {other:?}"),
        })
        .collect();

    assert_eq!(
        transitions,
        vec![
            PlacementAuditTransition::PlacementDecisionMade,
            PlacementAuditTransition::SegmentSealed,
            PlacementAuditTransition::SegmentPublishedCold,
            PlacementAuditTransition::ExtentReclaimed,
            PlacementAuditTransition::ColdMutationRejected,
        ]
    );

    let rejection = match &recorded[4].event {
        TraceEvent::PlacementAudit(trace) => trace,
        _ => unreachable!(),
    };
    assert!(!rejection.accepted);
    assert_eq!(
        rejection.transition,
        PlacementAuditTransition::ColdMutationRejected
    );
}
