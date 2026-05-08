use super::*;

#[test]
fn procedure_feedback_attaches_to_registered_procedure() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();

    let outcome = store
        .attach_procedure_feedback(feedback_for(1001, 1, 1))
        .expect("registered procedure accepts matching feedback");
    assert_eq!(outcome, RecordOutcome::Stored);

    // Idempotent re-attach is a deterministic Duplicate.
    let outcome = store
        .attach_procedure_feedback(feedback_for(1001, 1, 1))
        .expect("byte-identical feedback is idempotent");
    assert_eq!(outcome, RecordOutcome::Duplicate);
    assert_eq!(store.total_procedure_feedback(), 1);

    let visible = store.procedure_feedback_for_at(
        ProcedureId::new(1),
        andromeda_time::EngineTimestamp::from_unix_millis(50),
    );
    assert_eq!(visible.len(), 1);
    assert!(!visible[0].is_authoritative());
}

#[test]
fn procedure_feedback_rejects_unknown_procedure() {
    let mut store = ProcedureStore::new();
    let err = store
        .attach_procedure_feedback(feedback_for(1001, 99, 1))
        .unwrap_err();
    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Contract);
    assert!(err.message().contains("unknown procedure id"));
    assert_eq!(store.total_procedure_feedback(), 0);
}

#[test]
fn procedure_feedback_rejects_stats_version_mismatch_with_binding() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    // Registered binding uses StatsVersion::new(1); supply 7 instead.
    let err = store
        .attach_procedure_feedback(feedback_for(1001, 1, 7))
        .unwrap_err();
    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Contract);
    assert!(err.message().contains("stats version"));
    assert_eq!(store.total_procedure_feedback(), 0);
}

#[test]
fn procedure_feedback_rejects_conflicting_record_with_same_id_but_different_digest() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    store
        .attach_procedure_feedback(feedback_for(1001, 1, 1))
        .unwrap();

    let conflicting = ProcedureFeedback::new(
        crate::FeedbackId::new(1001).unwrap(),
        ProcedureId::new(1),
        Some([0xCD; 32]),
        StatsVersion::new(1),
        crate::CompletionEvidence {
            status: crate::CompletionStatus::Failed,
            completion_code: Some(7),
            row_count: None,
            durable_lsn: None,
        },
        make_window(10, 1_000),
    )
    .unwrap();

    let err = store.attach_procedure_feedback(conflicting).unwrap_err();
    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Contract);
    assert!(err.message().to_lowercase().contains("conflict"));
    assert_eq!(store.total_procedure_feedback(), 1);
}

#[test]
fn procedure_feedback_prune_expired_drops_only_expired_records() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    // Window 10..50 expires at t=50; window 10..1000 stays valid.
    let short = ProcedureFeedback::new(
        crate::FeedbackId::new(1).unwrap(),
        ProcedureId::new(1),
        None,
        StatsVersion::new(1),
        crate::CompletionEvidence {
            status: crate::CompletionStatus::Committed,
            completion_code: Some(0),
            row_count: Some(1),
            durable_lsn: Some(7),
        },
        make_window(10, 50),
    )
    .unwrap();
    store.attach_procedure_feedback(short).unwrap();
    store
        .attach_procedure_feedback(feedback_for(2, 1, 1))
        .unwrap();

    let removed = store
        .prune_expired_procedure_feedback(andromeda_time::EngineTimestamp::from_unix_millis(100));
    assert_eq!(removed, 1);
    assert_eq!(store.total_procedure_feedback(), 1);
}

#[test]
fn procedure_feedback_visibility_respects_validity_window_bounds() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();

    let future = ProcedureFeedback::new(
        crate::FeedbackId::new(3).unwrap(),
        ProcedureId::new(1),
        Some([0xAB; 32]),
        StatsVersion::new(1),
        crate::CompletionEvidence {
            status: crate::CompletionStatus::Committed,
            completion_code: Some(0),
            row_count: Some(1),
            durable_lsn: Some(9),
        },
        make_window(200, 400),
    )
    .unwrap();

    store.attach_procedure_feedback(future).unwrap();

    let before_issue = store.procedure_feedback_for_at(
        ProcedureId::new(1),
        andromeda_time::EngineTimestamp::from_unix_millis(199),
    );
    assert!(before_issue.is_empty());

    let at_issue = store.procedure_feedback_for_at(
        ProcedureId::new(1),
        andromeda_time::EngineTimestamp::from_unix_millis(200),
    );
    assert_eq!(at_issue.len(), 1);
    assert!(!at_issue[0].is_authoritative());
}
