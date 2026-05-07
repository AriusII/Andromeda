use crate::support::*;

#[test]
fn file_backed_durable_audit_replay_detects_checksum_chain_gap() {
    let path = temp_journal_path("checksum-chain-gap");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        6,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        7,
        admin_principal_binding(70, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            70,
            170,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    assert!(
        journal_text.contains("previous_chain_checksum="),
        "journal records carry explicit chain predecessor evidence"
    );
    assert!(
        journal_text.contains("chain_checksum="),
        "journal records carry explicit chain checksum evidence"
    );
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    fs::write(&path, format!("{}\n", lines[1])).expect("test can remove first chain record");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("removing a prior record breaks the checksum chain");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("checksum chain previous value mismatch"),
        "replay should identify the broken predecessor link"
    );

    let open_failure =
        FileDurableAuditWalSink::open(&path).expect_err("broken checksum chain blocks open");
    assert_eq!(
        open_failure.kind,
        DurableAuditFailureKind::CorruptionDetected
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_missing_chain_anchor_for_non_empty_journal() {
    let path = temp_journal_path("checksum-chain-missing-anchor");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        60,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append writes journal and chain anchor");

    fs::remove_file(journal_chain_anchor_path(&path)).expect("test can remove chain anchor");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("non-empty journal without a chain anchor must fail closed");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("chain anchor is required"),
        "replay should not silently accept journal records after anchor deletion"
    );

    let open_failure =
        FileDurableAuditWalSink::open(&path).expect_err("missing chain anchor blocks open");
    assert_eq!(
        open_failure.kind,
        DurableAuditFailureKind::CorruptionDetected
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_reordered_chain_records() {
    let path = temp_journal_path("checksum-chain-reorder");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        61,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        62,
        admin_principal_binding(62, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            62,
            162,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    fs::write(&path, format!("{}\n{}\n", lines[1], lines[0]))
        .expect("test can reorder journal records");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("reordered durable audit records break the checksum chain");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("checksum chain previous value mismatch"),
        "replay should reject records whose predecessor evidence does not match scan order"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_duplicate_lsn_even_when_rechained() {
    let path = temp_journal_path("checksum-chain-duplicate-lsn");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        69,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before duplicate LSN corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    let duplicate_payload = journal_payload_from_line(lines[0]);
    let duplicate_line =
        journal_line_with_chain(duplicate_payload, chain_checksum_from_line(lines[0]));
    fs::write(&path, format!("{}\n{}", lines[0], duplicate_line))
        .expect("test can append a rechained duplicate LSN");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("duplicate LSN must fail even when the checksum chain is syntactically valid");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("record LSNs must increase strictly"),
        "replay should reject duplicate LSNs after chain validation"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_inserted_chain_record() {
    let path = temp_journal_path("checksum-chain-insertion");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        69,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        70,
        admin_principal_binding(70, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            70,
            170,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    fs::write(&path, format!("{}\n{}\n{}\n", lines[0], lines[0], lines[1]))
        .expect("test can insert a stale replay record into the chain");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("inserted durable audit records must break predecessor evidence");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("checksum chain previous value mismatch"),
        "replay should reject inserted records whose predecessor evidence is stale"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_chain_checksum_alteration() {
    let path = temp_journal_path("checksum-chain-alteration");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        64,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let corrupted = journal_text.replacen("|chain_checksum=", "|chain_checksum=ffffffff", 1);
    fs::write(&path, corrupted).expect("test can alter chain checksum field");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("altered chain checksum is rejected");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("chain_checksum field")
            || failure.reason.contains("chain checksum mismatch"),
        "replay should identify the altered chain checksum field"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_forged_chain_tail_against_anchor() {
    let path = temp_journal_path("checksum-chain-forged-tail");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        65,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        66,
        admin_principal_binding(66, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            66,
            166,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    fs::write(&path, format!("{}\n", lines[0])).expect("test can remove the anchored chain tail");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("removing the tail record must not satisfy the persisted chain anchor");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("chain anchor last record LSN mismatch")
            || failure
                .reason
                .contains("chain anchor record count mismatch")
            || failure
                .reason
                .contains("chain anchor tail checksum mismatch"),
        "replay should identify the tail deletion against persisted anchor evidence"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_chain_anchor_checksum_alteration() {
    let path = temp_journal_path("checksum-anchor-alteration");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        76,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before anchor checksum corruption");

    let anchor_path = journal_chain_anchor_path(&path);
    let anchor_text = fs::read_to_string(&anchor_path).expect("chain anchor can be read");
    let (payload, _) = anchor_text
        .trim_end()
        .rsplit_once("|checksum=")
        .expect("chain anchor carries checksum evidence");
    fs::write(
        &anchor_path,
        format!("{payload}|checksum=ffffffffffffffff\n"),
    )
    .expect("test can alter chain anchor checksum evidence");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("altered chain anchor checksum is rejected");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("chain anchor checksum mismatch"),
        "replay should identify the altered chain anchor checksum field"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(anchor_path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_forged_chain_head_against_anchor() {
    let path = temp_journal_path("checksum-chain-forged-head");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        67,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        68,
        admin_principal_binding(68, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            68,
            168,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    let forged_head_payload = journal_payload_from_line(lines[1]);
    fs::write(&path, journal_line_with_chain(forged_head_payload, 0))
        .expect("test can forge a syntactically valid chain head from the second record");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("forged chain head must not satisfy persisted anchor evidence");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("chain anchor first record LSN mismatch")
            || failure
                .reason
                .contains("chain anchor record count mismatch"),
        "replay should identify the forged head against persisted anchor evidence"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_targeted_replay_validates_unmatched_chain_prefix() {
    let path = temp_journal_path("targeted-chain-prefix");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        8,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        9,
        admin_principal_binding(80, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            80,
            180,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let corrupted = journal_text.replace("SecurityDecision", "GenericAudit");
    assert_ne!(journal_text, corrupted, "test must corrupt the first line");
    fs::write(&path, corrupted).expect("test can corrupt journal in place");

    let failure = sink
        .replay(&DurableAuditReplayQuery {
            family: Some(DurableAuditEventFamily::AdminDecision),
            trace_id: Some(TraceId::new(180)),
            principal_id: Some("user:admin-audit".to_string()),
            ..DurableAuditReplayQuery::all()
        })
        .expect_err("targeted replay must validate unmatched prefix records");

    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("checksum mismatch")
            || failure.reason.contains("chain checksum mismatch"),
        "replay should fail because an earlier chain record was corrupted"
    );

    let _ = fs::remove_file(path);
}
