use crate::support::*;

#[test]
fn file_backed_durable_audit_replay_detects_checksum_corruption() {
    let path = temp_journal_path("checksum-corruption");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        4,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record contract is satisfied");

    sink.append_durable_audit_record(record)
        .expect("append succeeds before corruption");
    let corrupted = fs::read_to_string(&path)
        .expect("journal can be read")
        .replace("SecurityDecision", "GenericAudit");
    fs::write(&path, corrupted).expect("test can corrupt journal in place");

    let query_failure = sink
        .replay(&DurableAuditReplayQuery {
            family: Some(DurableAuditEventFamily::SecurityDecision),
            trace_id: Some(TraceId::new(90)),
            principal_id: Some("user:durable-audit".to_string()),
            lsn_range: Some(DurableAuditReplayLsnRange::new(1, u64::MAX)),
        })
        .expect_err("targeted replay inspection detects checksum corruption");
    assert_eq!(
        query_failure.kind,
        DurableAuditFailureKind::CorruptionDetected
    );

    let failure =
        FileDurableAuditWalSink::open(&path).expect_err("checksum mismatch blocks replay/open");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_legacy_journal_without_chain_fields() {
    let path = temp_journal_path("legacy-no-chain-fields");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        73,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before legacy rewrite");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let legacy_payload = journal_payload_from_line(
        journal_text
            .lines()
            .next()
            .expect("journal contains one current-format line"),
    );
    fs::write(&path, journal_line_with_checksum(legacy_payload))
        .expect("test can rewrite journal to legacy checksum-only format");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("legacy checksum-only journals are explicitly rejected");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("missing chain_checksum field"),
        "legacy rejection should name the missing chain evidence field"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_unsupported_journal_format_version() {
    let path = temp_journal_path("unsupported-format-version");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        74,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before format version corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let payload = journal_payload_from_line(
        journal_text
            .lines()
            .next()
            .expect("journal contains one current-format line"),
    );
    let unsupported_payload = payload.replacen(
        "andromeda-durable-audit-v2",
        "andromeda-durable-audit-v99",
        1,
    );
    assert_ne!(
        payload, unsupported_payload,
        "test corruption must change the durable audit journal format prefix"
    );
    fs::write(&path, journal_line_with_chain(&unsupported_payload, 0))
        .expect("test can rewrite journal with a valid checksum chain");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("unsupported durable audit journal versions fail closed");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("format version") && failure.reason.contains("supported versions"),
        "version rejection should expose explicit format-version evidence"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_truncated_record_tail() {
    let path = temp_journal_path("checksum-chain-tail-truncation");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        63,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before corruption");

    let mut journal_text = fs::read_to_string(&path).expect("journal can be read");
    assert!(
        journal_text.ends_with('\n'),
        "journal record is newline-delimited before corruption"
    );
    journal_text.pop();
    fs::write(&path, journal_text).expect("test can truncate final record delimiter");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("missing record delimiter is a truncation boundary");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("record-delimited"),
        "replay should surface truncation as an observable record delimiter failure"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_secret_journal_fields_without_leak() {
    let path = temp_journal_path("secret-journal-field");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        75,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before secret-bearing corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let payload = journal_payload_from_line(
        journal_text
            .lines()
            .next()
            .expect("journal contains one current-format line"),
    );
    let principal_field = format!("principal_id={}", hex_encode("user:durable-audit"));
    let secret_principal_field = format!(
        "principal_id={}",
        hex_encode("token=must-not-enter-journal")
    );
    let corrupted_payload = payload.replace(&principal_field, &secret_principal_field);
    assert_ne!(
        payload, corrupted_payload,
        "test corruption must inject secret-bearing principal evidence"
    );
    fs::write(&path, journal_line_with_chain(&corrupted_payload, 0))
        .expect("test can rewrite journal with a valid checksum chain");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("secret-bearing journal evidence must fail closed");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(failure.reason.contains("secret evidence"));
    assert!(
        !failure.reason.contains("must-not-enter-journal"),
        "durable audit failure evidence must not echo secret-bearing journal fields"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_non_ascii_journal_payload_without_panic() {
    let path = temp_journal_path("non-ascii-journal-payload");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        5,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record contract is satisfied");

    sink.append_durable_audit_record(record)
        .expect("append succeeds before corruption");
    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let (payload, _) = journal_text
        .trim_end()
        .rsplit_once("|checksum=")
        .expect("journal line has checksum field");
    let principal_field = format!("principal_id={}", hex_encode("user:durable-audit"));
    let corrupted_payload = payload.replace(&principal_field, "principal_id=€a");
    assert_ne!(
        payload, corrupted_payload,
        "test corruption must alter the principal field"
    );
    fs::write(&path, journal_line_with_checksum(&corrupted_payload))
        .expect("test can rewrite malformed journal");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("malformed non-ASCII journal payload is rejected as corruption");

    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("ASCII"),
        "journal parser should reject non-ASCII payloads before field decoding"
    );

    let _ = fs::remove_file(path);
}
