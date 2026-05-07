use crate::support::*;

#[test]
fn durable_audit_replay_filter_rejects_unsafe_inputs() {
    let zero_trace = DurableAuditReplayQuery {
        trace_id: Some(TraceId::new(0)),
        ..DurableAuditReplayQuery::all()
    };
    assert!(
        zero_trace
            .validate()
            .unwrap_err()
            .message()
            .contains("trace_id")
    );

    let empty_principal = DurableAuditReplayQuery {
        principal_id: Some(" \t".to_string()),
        ..DurableAuditReplayQuery::all()
    };
    assert!(
        empty_principal
            .validate()
            .unwrap_err()
            .message()
            .contains("principal filter")
    );

    let secret_principal = DurableAuditReplayQuery {
        principal_id: Some("private_key=must-not-enter-replay-filter".to_string()),
        ..DurableAuditReplayQuery::all()
    };
    assert!(
        secret_principal
            .validate()
            .unwrap_err()
            .message()
            .contains("secret evidence")
    );

    let invalid_lsn = DurableAuditReplayQuery {
        lsn_range: Some(DurableAuditReplayLsnRange::new(20, 10)),
        ..DurableAuditReplayQuery::all()
    };
    assert!(
        invalid_lsn
            .validate()
            .unwrap_err()
            .message()
            .contains("LSN range")
    );
}

#[test]
fn file_backed_durable_audit_sink_survives_restart_and_replays_targeted_index() {
    let path = temp_journal_path("restart-replay");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        3,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(),
    )
    .expect("durable audit record contract is satisfied");

    let report = sink
        .append_durable_audit_record(record)
        .expect("append is flushed before success");
    report.validate().expect("report proves durable audit WAL");
    assert_ne!(report.evidence.record_lsn, 0);
    assert_eq!(report.evidence.record_lsn, report.evidence.durable_lsn);
    assert_ne!(report.evidence.checksum, 0);

    drop(sink);
    let reopened = FileDurableAuditWalSink::open(&path).expect("journal reopens after restart");
    let replayed = reopened
        .replay(&DurableAuditReplayQuery {
            family: Some(DurableAuditEventFamily::SecurityDecision),
            trace_id: Some(TraceId::new(90)),
            principal_id: Some("user:durable-audit".to_string()),
            lsn_range: Some(DurableAuditReplayLsnRange::new(
                report.evidence.record_lsn,
                report.evidence.durable_lsn,
            )),
        })
        .expect("targeted replay scans the durable journal");

    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, report);
    assert_eq!(
        replayed[0].principal_binding.request_id,
        Some(RequestId::new(7))
    );
    assert_eq!(
        replayed[0].principal_binding.session_id,
        Some(SessionId::new(8))
    );
    assert_eq!(replayed[0].event_kind, "SecurityAudit");

    let journal_text = fs::read_to_string(&path).expect("journal is readable");
    assert!(!journal_text.contains("transaction_id="));
    assert!(!journal_text.contains("token="));

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_append_after_reopen_and_replay_continues_chain() {
    let path = temp_journal_path("append-after-replay");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        71,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    let first_report = sink
        .append_durable_audit_record(first)
        .expect("first append succeeds before restart");
    drop(sink);

    let mut reopened = FileDurableAuditWalSink::open(&path).expect("journal reopens");
    let replayed = reopened
        .replay(&DurableAuditReplayQuery::all())
        .expect("reopened journal replays before next append");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, first_report);

    let second = PendingDurableAuditRecord::new(
        72,
        admin_principal_binding(72, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            72,
            172,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    let second_report = reopened
        .append_durable_audit_record(second)
        .expect("append after replay succeeds with continued chain evidence");
    assert_eq!(
        second_report.evidence.record_lsn,
        first_report.evidence.record_lsn + 1,
        "append after replay must continue from the durable chain tail"
    );

    let replayed_after_append = reopened
        .replay(&DurableAuditReplayQuery::all())
        .expect("journal remains replayable after append continuation");
    assert_eq!(replayed_after_append.len(), 2);
    assert_eq!(replayed_after_append[0].report, first_report);
    assert_eq!(replayed_after_append[1].report, second_report);

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert!(
        lines[1].contains("previous_chain_checksum="),
        "continued append must persist predecessor chain evidence"
    );

    let _ = fs::remove_file(path);
}
