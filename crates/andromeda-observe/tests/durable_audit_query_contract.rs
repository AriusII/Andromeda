use andromeda_observe::{
    DurableAuditEventFamily, DurableAuditPrincipalBinding, DurableAuditReplayBehavior,
    DurableAuditReplayRecord, DurableAuditRetentionBoundary, DurableAuditSinkReport,
    DurableAuditTraceQueryRow, DurableAuditTraceQuerySource, DurableAuditWalEvidence, EventId,
    Permission, SecurityPolicyVersionEvidence, SurfaceScope, TraceEventFamily, TraceId,
    TraceQueryFilter, TraceQueryLsnRange, TraceQuerySpec,
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
            identity: andromeda_observe::DurableAuditRecordIdentity {
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

fn expected_trace_family(
    family: DurableAuditEventFamily,
    event_kind: &str,
) -> Option<TraceEventFamily> {
    match family {
        DurableAuditEventFamily::SecurityDecision => Some(TraceEventFamily::SecurityAudit),
        DurableAuditEventFamily::AdminDecision
        | DurableAuditEventFamily::HadrDecision
        | DurableAuditEventFamily::BackupDecision
        | DurableAuditEventFamily::RestoreDecision
        | DurableAuditEventFamily::ForensicDecision
        | DurableAuditEventFamily::GenericAudit => Some(TraceEventFamily::AdminAudit),
        DurableAuditEventFamily::AdmissionDecision => Some(TraceEventFamily::Protocol),
        DurableAuditEventFamily::CatalogDecision => Some(TraceEventFamily::ManifestCatalog),
        DurableAuditEventFamily::RecoveryDecision => match event_kind {
            "WalAppend" | "WalFlush" | "CommitVisible" | "RollbackDurable"
            | "CorruptionBoundary" => Some(TraceEventFamily::Wal),
            "RecoveryStartup" => Some(TraceEventFamily::Recovery),
            _ => None,
        },
    }
}

#[test]
fn durable_audit_trace_inspection_source_filters_replay_records_with_common_spec() {
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

    let mut spec = TraceQuerySpec::new(TraceQueryFilter {
        family: Some(TraceEventFamily::Wal),
        principal: Some("user:storage".to_string()),
        lsn_range: Some(TraceQueryLsnRange::new(15, 25)),
        ..TraceQueryFilter::default()
    });
    spec.include_total_count = true;

    let result = source
        .inspect(&spec)
        .expect("durable replay records are inspectable through common trace filters");

    assert_eq!(result.metadata.returned_rows, 1);
    assert_eq!(result.metadata.total_matching_rows, Some(1));
    assert!(result.metadata.ordered_by_event_id_ascending);
    assert_eq!(result.rows[0].event_id, EventId::new(2));
    assert_eq!(result.rows[0].family, TraceEventFamily::Wal);
    assert_eq!(
        result.rows[0].durable_audit_family,
        DurableAuditEventFamily::RecoveryDecision
    );
    assert_eq!(result.rows[0].event_kind, "WalFlush");

    let recovery_spec = TraceQuerySpec::new(TraceQueryFilter {
        family: Some(TraceEventFamily::Recovery),
        trace_id: Some(TraceId::new(92)),
        ..TraceQueryFilter::default()
    });
    let recovery_result = source
        .inspect(&recovery_spec)
        .expect("event_kind evidence disambiguates recovery startup from WAL replay records");
    assert_eq!(recovery_result.metadata.returned_rows, 1);
    assert_eq!(recovery_result.rows[0].event_kind, "RecoveryStartup");
}

#[test]
fn durable_audit_trace_inspection_reports_when_replay_order_is_not_event_id_order() {
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

    let mut spec = TraceQuerySpec::new(TraceQueryFilter {
        principal: Some("user:ordered-replay".to_string()),
        ..TraceQueryFilter::default()
    });
    spec.include_total_count = true;

    let result = source
        .inspect(&spec)
        .expect("replay inspection preserves journal order for durable audit records");

    assert_eq!(result.metadata.returned_rows, 2);
    assert_eq!(result.metadata.total_matching_rows, Some(2));
    assert!(
        !result.metadata.ordered_by_event_id_ascending,
        "durable replay order is WAL/journal order, not an event_id ordering guarantee"
    );
    assert_eq!(result.rows[0].event_id, EventId::new(20));
    assert_eq!(result.rows[0].record_lsn, 10);
    assert_eq!(result.rows[1].event_id, EventId::new(10));
    assert_eq!(result.rows[1].record_lsn, 20);

    let mut paged_spec = TraceQuerySpec::new(TraceQueryFilter {
        principal: Some("user:ordered-replay".to_string()),
        ..TraceQueryFilter::default()
    });
    paged_spec.limit = 1;
    paged_spec.offset = 1;
    let paged = source
        .inspect(&paged_spec)
        .expect("pagination must not hide non-event-id replay ordering metadata");

    assert_eq!(paged.metadata.returned_rows, 1);
    assert!(!paged.metadata.ordered_by_event_id_ascending);
    assert_eq!(paged.rows[0].event_id, EventId::new(10));
    assert_eq!(paged.rows[0].record_lsn, 20);
}

#[test]
fn durable_audit_trace_inspection_mapping_covers_all_durable_families() {
    let records = DurableAuditEventFamily::ALL
        .iter()
        .enumerate()
        .map(|(offset, family)| {
            let event_id = 100 + offset as u128;
            let event_kind = if *family == DurableAuditEventFamily::RecoveryDecision {
                "RecoveryStartup"
            } else {
                "DurableAuditFamilyCoverage"
            };
            replay_record(
                event_id,
                2_000 + event_id,
                *family,
                event_kind,
                "user:family-coverage",
                1_000 + offset as u64,
            )
        })
        .collect::<Vec<_>>();
    let source = DurableAuditTraceQuerySource::new(&records);

    let result = source
        .inspect(&TraceQuerySpec::new(TraceQueryFilter::default()))
        .expect("all durable audit families are inspectable through the durable adapter");

    assert_eq!(result.rows.len(), DurableAuditEventFamily::ALL.len());
    for row in &result.rows {
        assert_eq!(
            Some(row.family),
            expected_trace_family(row.durable_audit_family, &row.event_kind)
        );
        assert!(
            DurableAuditEventFamily::ALL.contains(&row.durable_audit_family),
            "durable audit inspection row must preserve a known durable family"
        );
    }
}

#[test]
fn durable_audit_trace_inspection_source_rejects_filters_not_carried_by_journal() {
    let records = vec![replay_record(
        1,
        90,
        DurableAuditEventFamily::SecurityDecision,
        "SecurityAudit",
        "user:durable-audit",
        10,
    )];
    let source = DurableAuditTraceQuerySource::new(&records);

    let spec = TraceQuerySpec::new(TraceQueryFilter {
        catalog_version: Some(CatalogVersion::new(7)),
        ..TraceQueryFilter::default()
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
fn durable_audit_trace_inspection_source_rejects_secret_principal_filter() {
    let records = vec![replay_record(
        1,
        90,
        DurableAuditEventFamily::SecurityDecision,
        "SecurityAudit",
        "user:durable-audit",
        10,
    )];
    let source = DurableAuditTraceQuerySource::new(&records);

    let spec = TraceQuerySpec::new(TraceQueryFilter {
        principal: Some("x-api-key must-not-enter-query-path".to_string()),
        ..TraceQueryFilter::default()
    });

    let error = source
        .inspect(&spec)
        .expect_err("durable audit inspection rejects secret-bearing principal filters");
    assert!(
        error
            .message()
            .contains("principal filter must not contain secret evidence")
    );
}

#[test]
fn durable_audit_trace_inspection_row_rejects_secret_replay_fields_without_leak() {
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

#[test]
fn durable_audit_trace_inspection_source_rejects_unknown_recovery_event_kind() {
    let records = vec![replay_record(
        4,
        94,
        DurableAuditEventFamily::RecoveryDecision,
        "UnexpectedRecoveryString",
        "user:storage",
        40,
    )];
    let source = DurableAuditTraceQuerySource::new(&records);

    let error = source
        .inspect(&TraceQuerySpec::new(TraceQueryFilter::default()))
        .expect_err("unknown recovery event_kind evidence must fail closed");

    assert!(
        error.message().contains("unknown event_kind evidence"),
        "durable recovery replay projection must reject opaque event kinds"
    );
}
