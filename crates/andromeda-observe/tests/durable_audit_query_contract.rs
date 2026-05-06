use andromeda_core::{CatalogVersion, RequestId, SessionId};
use andromeda_observe::{
    DurableAuditEventFamily, DurableAuditPrincipalBinding, DurableAuditReplayBehavior,
    DurableAuditReplayRecord, DurableAuditRetentionBoundary, DurableAuditSinkReport,
    DurableAuditTraceQuerySource, DurableAuditWalEvidence, EventId, Permission, SurfaceScope,
    TraceEventFamily, TraceId, TraceQueryFilter, TraceQueryLsnRange, TraceQuerySpec,
};

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
            request_id: Some(RequestId::new(event_id as u64)),
            session_id: Some(SessionId::new((event_id + 100) as u64)),
        },
        event_kind: event_kind.to_string(),
    }
}

fn expected_trace_family(family: DurableAuditEventFamily, event_kind: &str) -> TraceEventFamily {
    match family {
        DurableAuditEventFamily::SecurityDecision => TraceEventFamily::SecurityAudit,
        DurableAuditEventFamily::AdminDecision
        | DurableAuditEventFamily::HadrDecision
        | DurableAuditEventFamily::BackupDecision
        | DurableAuditEventFamily::RestoreDecision
        | DurableAuditEventFamily::ForensicDecision
        | DurableAuditEventFamily::GenericAudit => TraceEventFamily::AdminAudit,
        DurableAuditEventFamily::AdmissionDecision => TraceEventFamily::Protocol,
        DurableAuditEventFamily::CatalogDecision => TraceEventFamily::ManifestCatalog,
        DurableAuditEventFamily::RecoveryDecision => match event_kind {
            "WalAppend" | "WalFlush" | "CommitVisible" | "RollbackDurable"
            | "CorruptionBoundary" => TraceEventFamily::Wal,
            _ => TraceEventFamily::Recovery,
        },
    }
}

#[test]
fn durable_audit_trace_query_source_filters_replay_records_with_common_query_spec() {
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
        .query(&spec)
        .expect("durable replay records are queryable through common trace filters");

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
        .query(&recovery_spec)
        .expect("event_kind evidence disambiguates recovery startup from WAL replay records");
    assert_eq!(recovery_result.metadata.returned_rows, 1);
    assert_eq!(recovery_result.rows[0].event_kind, "RecoveryStartup");
}

#[test]
fn durable_audit_trace_query_mapping_covers_all_durable_families() {
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
        .query(&TraceQuerySpec::new(TraceQueryFilter::default()))
        .expect("all durable audit families are queryable through the durable query adapter");

    assert_eq!(result.rows.len(), DurableAuditEventFamily::ALL.len());
    for row in &result.rows {
        assert_eq!(
            row.family,
            expected_trace_family(row.durable_audit_family, &row.event_kind)
        );
        assert!(
            DurableAuditEventFamily::ALL.contains(&row.durable_audit_family),
            "durable audit query row must preserve a known durable family"
        );
    }
}

#[test]
fn durable_audit_trace_query_source_rejects_filters_not_carried_by_journal() {
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
        .query(&spec)
        .expect_err("durable audit journal lacks catalog correlation");
    assert!(
        error
            .message()
            .contains("durable audit journal does not carry catalog correlation")
    );
}

#[test]
fn durable_audit_trace_query_source_rejects_secret_principal_filter() {
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
        principal: Some("token=must-not-enter-query-path".to_string()),
        ..TraceQueryFilter::default()
    });

    let error = source
        .query(&spec)
        .expect_err("durable audit query rejects secret-bearing principal filters");
    assert!(
        error
            .message()
            .contains("principal filter must not contain secret evidence")
    );
}
