use andromeda_procedure_store::{
    AuditCorrelation, AuditCorrelationId, EvidenceDigest, FeedbackId, InvocationEvidenceKind,
    InvocationEvidenceMarker, InvocationFeedback, InvocationHistoryRecord, InvocationId,
    InvocationIdentity, InvocationMetricKind, InvocationMetrics, InvocationStatus, ProcedureId,
    ProcedureStorePrimitiveError, RegressionSeverity, RegressionSignal, RegressionThresholdBps,
};

fn identity() -> InvocationIdentity {
    InvocationIdentity::new(
        InvocationId::new(101).unwrap(),
        ProcedureId::new(202).unwrap(),
    )
}

fn digest(byte: u8) -> EvidenceDigest {
    EvidenceDigest::new([byte; EvidenceDigest::LEN]).unwrap()
}

#[test]
fn invocation_history_preserves_status_metrics_and_audit_correlation_as_evidence() {
    let identity = identity();
    let metrics = InvocationMetrics::new(42, 9, 2, 512, 16)
        .with_rows_returned(7)
        .with_read_counters(12, 1, 11)
        .with_spill_bytes(8);
    let evidence = InvocationEvidenceMarker::new(InvocationEvidenceKind::Metrics, digest(0xA1));
    let audit_correlation_id = AuditCorrelationId::new(7_001).unwrap();
    let history = [
        InvocationHistoryRecord::new(
            identity,
            InvocationStatus::Admitted,
            InvocationEvidenceMarker::new(InvocationEvidenceKind::Admission, digest(0xA0)),
        ),
        InvocationHistoryRecord::new(identity, InvocationStatus::Committed, evidence)
            .with_metrics(metrics)
            .with_audit_correlation_id(audit_correlation_id),
    ];

    assert!(!history[0].is_terminal());
    assert_eq!(history[1].identity(), identity);
    assert_eq!(history[1].status(), InvocationStatus::Committed);
    assert_eq!(history[1].evidence().kind, InvocationEvidenceKind::Metrics);
    assert_eq!(history[1].metrics(), Some(metrics));
    assert_eq!(
        history[1].audit_correlation_id(),
        Some(audit_correlation_id)
    );
    assert!(history[1].is_terminal());
    assert!(!history[1].is_durable_truth());
    assert!(!history[1].can_select_plan_alone());
}

#[test]
fn feedback_accepts_only_terminal_status_and_remains_advisory() {
    let identity = identity();
    let metrics = InvocationMetrics::new(11, 3, 0, 0, 0);

    assert_eq!(
        FeedbackId::new(0).unwrap_err(),
        ProcedureStorePrimitiveError::ZeroFeedbackId
    );
    assert_eq!(
        InvocationFeedback::new(
            FeedbackId::new(1).unwrap(),
            identity,
            InvocationStatus::Started,
            metrics,
            digest(0xB0),
        )
        .unwrap_err(),
        ProcedureStorePrimitiveError::NonTerminalFeedbackStatus
    );

    let feedback = InvocationFeedback::new(
        FeedbackId::new(2).unwrap(),
        identity,
        InvocationStatus::RolledBack,
        metrics,
        digest(0xB1),
    )
    .unwrap()
    .with_audit_correlation_id(AuditCorrelationId::new(9_001).unwrap());

    assert_eq!(feedback.feedback_id().get(), 2);
    assert_eq!(feedback.status(), InvocationStatus::RolledBack);
    assert_eq!(feedback.metrics(), metrics);
    assert!(feedback.is_observed_feedback());
    assert!(!feedback.is_authoritative());
    assert!(!feedback.is_durable_truth());
    assert!(!feedback.can_select_plan_alone());
}

#[test]
fn regression_signal_is_finite_bounded_and_not_plan_authority() {
    let threshold = RegressionThresholdBps::new(500).unwrap();
    let signal = RegressionSignal::new(
        identity(),
        InvocationMetricKind::DurationMillis,
        100,
        108,
        threshold,
        digest(0xC0),
    );
    let zero_baseline = RegressionSignal::new(
        identity(),
        InvocationMetricKind::WalBytes,
        0,
        1,
        threshold,
        digest(0xC1),
    );

    assert_eq!(
        RegressionThresholdBps::new(10_001).unwrap_err(),
        ProcedureStorePrimitiveError::RegressionThresholdOutOfRange
    );
    assert_eq!(signal.regression_bps(), 800);
    assert!(signal.is_regression());
    assert_eq!(signal.severity(), RegressionSeverity::Advisory);
    assert_eq!(zero_baseline.regression_bps(), 10_000);
    assert_eq!(zero_baseline.severity(), RegressionSeverity::Critical);
    assert!(!signal.is_authoritative());
    assert!(!signal.is_durable_truth());
    assert!(!signal.can_select_plan_alone());
}

#[test]
fn audit_correlation_binds_audit_and_decision_trace_digests_without_storage_truth() {
    assert_eq!(
        AuditCorrelationId::new(0).unwrap_err(),
        ProcedureStorePrimitiveError::ZeroAuditCorrelationId
    );

    let correlation = AuditCorrelation::new(
        identity(),
        AuditCorrelationId::new(44).unwrap(),
        digest(0xD0),
    )
    .with_decision_trace_digest(digest(0xD1));

    assert_eq!(correlation.identity(), identity());
    assert_eq!(correlation.correlation_id().get(), 44);
    assert_eq!(correlation.audit_digest(), digest(0xD0));
    assert_eq!(correlation.decision_trace_digest(), Some(digest(0xD1)));
    assert!(!correlation.is_durable_truth());
}
