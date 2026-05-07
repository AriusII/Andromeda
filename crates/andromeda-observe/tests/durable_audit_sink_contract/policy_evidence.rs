use crate::support::*;

#[test]
fn pending_durable_audit_record_binds_identity_family_principal_and_session() {
    let record = PendingDurableAuditRecord::new(
        1,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(),
    )
    .expect("durable audit record contract is satisfied");

    assert_eq!(record.identity.event_id, EventId::new(11));
    assert_eq!(record.identity.trace_id, TraceId::new(90));
    assert_eq!(
        record.identity.family,
        DurableAuditEventFamily::SecurityDecision
    );
    assert_eq!(record.principal_binding.request_id, Some(RequestId::new(7)));
    assert_eq!(record.principal_binding.session_id, Some(SessionId::new(8)));
    record.validate().expect("record remains valid");
}

#[test]
fn admin_decision_fails_closed_when_audit_sink_fails() {
    let record = PendingDurableAuditRecord::new(
        20,
        admin_principal_binding(20, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            20,
            120,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("admin durable audit record is valid before sink failure");
    assert_eq!(
        record.identity.family,
        DurableAuditEventFamily::AdminDecision
    );

    let mut sink = FailingDurableAuditWalSink;
    let failure = DurableAuditDecisionGate::new(DurableAuditEventFamily::AdminDecision)
        .append_and_prove(&mut sink, record)
        .expect_err("visible admin decision fails closed when audit WAL flush fails");

    assert_eq!(failure.kind, DurableAuditFailureKind::WalFlushRejected);
    assert!(failure.requires_fail_closed());
}

#[test]
fn permissioned_admin_records_fail_closed_without_policy_evidence() {
    let mut missing_policy = admin_principal_binding(24, Permission::InspectPlans);
    missing_policy.policy_version = None;
    let err = PendingDurableAuditRecord::new(
        24,
        missing_policy,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            24,
            124,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect_err("permissioned admin durable audit records require policy evidence");

    assert!(
        err.message()
            .contains("permissioned critical AdminDecision records")
    );

    let mut zero_policy = admin_principal_binding(29, Permission::InspectPlans);
    zero_policy.policy_version = Some(SecurityPolicyVersionEvidence {
        policy_version: 0,
        policy_digest: "sha256:9999999999999999999999999999999999999999999999999999999999999999"
            .to_string(),
    });
    let err = PendingDurableAuditRecord::new(
        29,
        zero_policy,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            29,
            129,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect_err("zero policy version evidence must fail closed");
    assert!(
        err.message()
            .contains("policy version evidence must be non-zero")
    );
}

#[test]
fn permissioned_hadr_records_require_policy_evidence_before_visible_decision() {
    let path = temp_journal_path("hadr-policy-evidence-gate");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let mut missing_policy = hadr_principal_binding(25, Permission::ClusterPromote);
    missing_policy.policy_version = None;
    let rejected = PendingDurableAuditRecord::new(
        25,
        missing_policy,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        hadr_envelope(
            25,
            125,
            AdminOperation::ClusterPromote,
            Permission::ClusterPromote,
        ),
    )
    .expect_err("HADR durable audit records require policy evidence");
    assert!(
        rejected
            .message()
            .contains("permissioned critical HadrDecision records")
    );

    let record = PendingDurableAuditRecord::new(
        26,
        hadr_principal_binding(26, Permission::ClusterPromote),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        hadr_envelope(
            26,
            126,
            AdminOperation::ClusterPromote,
            Permission::ClusterPromote,
        ),
    )
    .expect("HADR record with policy evidence is durable-audit eligible");
    let proof = DurableAuditDecisionGate::new(DurableAuditEventFamily::HadrDecision)
        .append_and_prove(&mut sink, record)
        .expect("HADR visible decision is proved by durable audit policy evidence");

    assert_eq!(
        proof.report.identity.family,
        DurableAuditEventFamily::HadrDecision
    );
    assert!(
        proof
            .principal_binding
            .policy_version
            .as_ref()
            .is_some_and(SecurityPolicyVersionEvidence::has_version_evidence)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn recovery_replay_policy_gate_only_applies_to_permissioned_records() {
    let base_report = DurableAuditSinkReport {
        identity: andromeda_observe::DurableAuditRecordIdentity {
            event_id: EventId::new(27),
            trace_id: TraceId::new(127),
            family: DurableAuditEventFamily::RecoveryDecision,
            sequence_number: 27,
        },
        evidence: DurableAuditWalEvidence {
            record_lsn: 27,
            durable_lsn: 27,
            checksum: 0xA11D_1701,
        },
        replay_behavior: DurableAuditReplayBehavior::ForensicOnly,
        retention: DurableAuditRetentionBoundary::ForensicHold,
    };
    let observational_recovery = DurableAuditReplayRecord {
        report: base_report,
        principal_binding: DurableAuditPrincipalBinding {
            principal_id: "svc:recovery-observer".to_string(),
            certificate_fingerprint: None,
            surface: Some(SurfaceScope::MonitoringAgent),
            permission: None,
            policy_version: None,
            request_id: Some(RequestId::new(7)),
            session_id: Some(SessionId::new(8)),
        },
        event_kind: "RecoveryStartup".to_string(),
    };
    observational_recovery
        .validate()
        .expect("non-permissioned recovery replay records remain compatible");

    let permissioned_without_policy = DurableAuditReplayRecord {
        report: DurableAuditSinkReport {
            identity: andromeda_observe::DurableAuditRecordIdentity {
                event_id: EventId::new(28),
                trace_id: TraceId::new(128),
                family: DurableAuditEventFamily::RecoveryDecision,
                sequence_number: 28,
            },
            ..base_report
        },
        principal_binding: DurableAuditPrincipalBinding {
            permission: Some(Permission::Restore),
            surface: Some(SurfaceScope::BackupAgent),
            ..observational_recovery.principal_binding
        },
        event_kind: "RecoveryStartup".to_string(),
    };
    let err = permissioned_without_policy
        .validate()
        .expect_err("permissioned recovery replay records fail closed without policy evidence");
    assert!(
        err.message()
            .contains("permissioned critical RecoveryDecision records")
    );
}

#[test]
fn visible_decision_gate_rejects_non_visible_family_before_journal_append() {
    let path = temp_journal_path("non-visible-gate-no-append");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        23,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(),
    )
    .expect("security durable audit record is valid");

    let failure = DurableAuditDecisionGate::new(DurableAuditEventFamily::AdmissionDecision)
        .append_and_prove(&mut sink, record)
        .expect_err("non-visible decision families must be rejected before append");

    assert_eq!(failure.kind, DurableAuditFailureKind::ValidationRejected);
    assert!(failure.reason.contains("visible decision family"));
    assert!(
        fs::read_to_string(&path)
            .expect("journal remains readable")
            .is_empty(),
        "visible decision gate must not append records for non-visible families"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn security_allow_requires_durable_audit_evidence() {
    let record = PendingDurableAuditRecord::new(
        21,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(),
    )
    .expect("security durable audit record is valid");
    let missing_flush = DurableAuditSinkReport {
        identity: record.identity,
        evidence: DurableAuditWalEvidence {
            record_lsn: 0,
            durable_lsn: 0,
            checksum: 0,
        },
        replay_behavior: DurableAuditReplayBehavior::RebuildDecisionIndex,
        retention: DurableAuditRetentionBoundary::SecurityPolicy,
    };

    let failure = DurableAuditDecisionGate::new(DurableAuditEventFamily::SecurityDecision)
        .prove_visible_decision(missing_flush, principal_binding())
        .expect_err("security allow must not become visible without durable audit evidence");

    assert_eq!(failure.kind, DurableAuditFailureKind::ValidationRejected);
    assert!(failure.requires_fail_closed());
    assert!(failure.reason.contains("flushed WAL evidence"));
}

#[test]
fn durable_audit_contract_rejects_secret_principal_evidence_and_zero_sequence() {
    let secret_binding = DurableAuditPrincipalBinding {
        principal_id: "credential=must-not-enter-audit".to_string(),
        ..principal_binding()
    };

    assert!(
        PendingDurableAuditRecord::new(
            1,
            secret_binding,
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::ForensicOnly,
            security_envelope(),
        )
        .unwrap_err()
        .message()
        .contains("secret evidence")
    );

    assert!(
        PendingDurableAuditRecord::new(
            0,
            principal_binding(),
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::ForensicOnly,
            security_envelope(),
        )
        .unwrap_err()
        .message()
        .contains("sequence_number")
    );
}

#[test]
fn durable_audit_principal_binding_rejects_impossible_permission_and_half_correlation() {
    let impossible_permission = DurableAuditPrincipalBinding {
        surface: Some(SurfaceScope::Application),
        permission: Some(Permission::ManageSecurity),
        ..principal_binding()
    };
    assert!(
        impossible_permission
            .validate()
            .unwrap_err()
            .message()
            .contains("surface must permit permission")
    );

    let half_correlated = DurableAuditPrincipalBinding {
        session_id: None,
        ..principal_binding()
    };
    assert!(
        half_correlated
            .validate()
            .unwrap_err()
            .message()
            .contains("present together")
    );
}
