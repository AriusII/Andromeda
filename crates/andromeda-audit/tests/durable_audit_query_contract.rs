use andromeda_audit::{
    DurableAuditEventFamily, DurableAuditPrincipalBinding, DurableAuditRecordIdentity,
    DurableAuditReplayBehavior, DurableAuditReplayRecord, DurableAuditRetentionBoundary,
    DurableAuditSinkReport, DurableAuditTraceFamily, DurableAuditTraceQueryFilter,
    DurableAuditTraceQueryLsnRange, DurableAuditTraceQueryRow, DurableAuditTraceQuerySource,
    DurableAuditTraceQuerySpec, DurableAuditWalEvidence, EventId, Permission,
    SecurityPolicyVersionEvidence, SurfaceScope, TraceId,
};
use andromeda_types::{CatalogVersion, RequestId, SessionId};

fn replay_record(
    event_id: u128,
    trace_id: u128,
    family: DurableAuditEventFamily,
    event_kind: &str,
    principal_id: &str,
    record_lsn: u64,
) -> DurableAuditReplayRecord {
    DurableAuditReplayRecord {
        report: DurableAuditSinkReport {
            identity: DurableAuditRecordIdentity {
                event_id: EventId::new(event_id),
                trace_id: TraceId::new(trace_id),
                family,
                sequence_number: event_id as u64,
            },
            evidence: DurableAuditWalEvidence {
                record_lsn,
                durable_lsn: record_lsn,
                checksum: 0xA11D1700 + record_lsn,
            },
            replay_behavior: DurableAuditReplayBehavior::ForensicOnly,
            retention: DurableAuditRetentionBoundary::ForensicHold,
        },
        principal_binding: DurableAuditPrincipalBinding {
            principal_id: principal_id.to_string(),
            certificate_fingerprint: Some(format!("sha256:fingerprint-{event_id}")),
            surface: Some(SurfaceScope::Administration),
            permission: Some(Permission::InspectPlans),
            policy_version: matches!(
                family,
                DurableAuditEventFamily::SecurityDecision
                    | DurableAuditEventFamily::AdminDecision
                    | DurableAuditEventFamily::CatalogDecision
                    | DurableAuditEventFamily::HadrDecision
                    | DurableAuditEventFamily::BackupDecision
                    | DurableAuditEventFamily::RestoreDecision
                    | DurableAuditEventFamily::ForensicDecision
                    | DurableAuditEventFamily::RecoveryDecision
            )
            .then(SecurityPolicyVersionEvidence::bootstrap_v0),
            request_id: Some(RequestId::new(event_id as u64)),
            session_id: Some(SessionId::new((event_id + 100) as u64)),
        },
        event_kind: event_kind.to_string(),
    }
}

#[test]
fn durable_audit_owner_query_filters_replay_records_without_event_envelopes() {
    let records = vec![
        replay_record(
            1,
            90,
            DurableAuditEventFamily::SecurityDecision,
            "SecurityAudit",
            "user:durable-audit",
            10,
        ),
        replay_record(
            2,
            91,
            DurableAuditEventFamily::RecoveryDecision,
            "WalFlush",
            "user:storage",
            20,
        ),
        replay_record(
            3,
            92,
            DurableAuditEventFamily::RecoveryDecision,
            "RecoveryStartup",
            "user:storage",
            30,
        ),
    ];
    let source = DurableAuditTraceQuerySource::new(&records);

    let mut spec = DurableAuditTraceQuerySpec::new(DurableAuditTraceQueryFilter {
        family: Some(DurableAuditTraceFamily::Wal),
        principal: Some("user:storage".to_string()),
        lsn_range: Some(DurableAuditTraceQueryLsnRange::new(15, 25)),
        ..DurableAuditTraceQueryFilter::default()
    });
    spec.include_total_count = true;

    let result = source
        .inspect(&spec)
        .expect("durable replay records are inspectable through audit-owned filters");

    assert_eq!(result.metadata.returned_rows, 1);
    assert_eq!(result.metadata.total_matching_rows, Some(1));
    assert_eq!(result.rows[0].event_id, EventId::new(2));
    assert_eq!(result.rows[0].family, DurableAuditTraceFamily::Wal);
    assert_eq!(
        result.rows[0].durable_audit_family,
        DurableAuditEventFamily::RecoveryDecision
    );
}

#[test]
fn durable_audit_owner_query_preserves_replay_order_metadata() {
    let records = vec![
        replay_record(
            20,
            120,
            DurableAuditEventFamily::AdminDecision,
            "AdminOperation",
            "user:ordered-replay",
            10,
        ),
        replay_record(
            10,
            110,
            DurableAuditEventFamily::AdminDecision,
            "AdminOperation",
            "user:ordered-replay",
            20,
        ),
    ];
    let source = DurableAuditTraceQuerySource::new(&records);

    let mut spec = DurableAuditTraceQuerySpec::new(DurableAuditTraceQueryFilter {
        principal: Some("user:ordered-replay".to_string()),
        ..DurableAuditTraceQueryFilter::default()
    });
    spec.include_total_count = true;

    let result = source
        .inspect(&spec)
        .expect("replay inspection preserves journal order");

    assert_eq!(result.metadata.returned_rows, 2);
    assert_eq!(result.metadata.total_matching_rows, Some(2));
    assert!(!result.metadata.ordered_by_event_id_ascending);
    assert_eq!(result.rows[0].event_id, EventId::new(20));
    assert_eq!(result.rows[1].event_id, EventId::new(10));
}

#[test]
fn durable_audit_owner_query_rejects_filters_not_carried_by_journal() {
    let records = vec![replay_record(
        1,
        90,
        DurableAuditEventFamily::SecurityDecision,
        "SecurityAudit",
        "user:durable-audit",
        10,
    )];
    let source = DurableAuditTraceQuerySource::new(&records);

    let spec = DurableAuditTraceQuerySpec::new(DurableAuditTraceQueryFilter {
        catalog_version: Some(CatalogVersion::new(7)),
        ..DurableAuditTraceQueryFilter::default()
    });

    let error = source
        .inspect(&spec)
        .expect_err("durable audit journal lacks catalog correlation");
    assert!(
        error
            .message()
            .contains("durable audit journal does not carry catalog correlation")
    );
}

#[test]
fn durable_audit_owner_query_rejects_secret_replay_fields_without_leak() {
    let record = replay_record(
        5,
        95,
        DurableAuditEventFamily::SecurityDecision,
        "token=must-not-appear",
        "user:durable-audit",
        50,
    );

    let error = DurableAuditTraceQueryRow::from_replay_record(&record)
        .expect_err("secret-bearing replay evidence must not become an inspection row");

    assert!(error.message().contains("secret evidence"));
    assert!(
        !error.message().contains("must-not-appear"),
        "durable audit inspection errors must not echo secret-bearing values"
    );
}
