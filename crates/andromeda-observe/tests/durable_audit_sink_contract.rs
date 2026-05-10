use andromeda_audit::{
    AdminOperation, AdminOperationTrace, CertificateIdentity, DurableAuditEventFamily,
    DurableAuditFailureKind, DurableAuditPrincipalBinding, DurableAuditReplayBehavior,
    DurableAuditRetentionBoundary, DurableAuditSinkFailure, DurableAuditSinkReport,
    DurableAuditWalEvidence, DurableAuditWalSink, Permission, SecurityAuditOutcome,
    SecurityAuditTrace, SecurityPolicyVersionEvidence, SurfaceScope, UserPrincipal,
    UserPrincipalKind,
};
use andromeda_observability::{EventCorrelation, EventId, TraceId};
use andromeda_observe::{EventEnvelope, PendingDurableAuditRecord, TraceEvent};
use andromeda_types::{RequestId, SessionId};

#[test]
fn pending_durable_audit_record_projects_security_envelope_to_audit_append_record() {
    let record = PendingDurableAuditRecord::new(
        1,
        security_principal_binding(11, "user:durable-audit"),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(11, 90, "user:durable-audit"),
    )
    .expect("observe adapter builds a durable audit record from an eligible envelope");

    assert_eq!(record.identity.event_id, EventId::new(11));
    assert_eq!(record.identity.trace_id, TraceId::new(90));
    assert_eq!(
        record.identity.family,
        DurableAuditEventFamily::SecurityDecision
    );

    let append_record = record.into_append_record();
    assert_eq!(append_record.event_kind, "SecurityAudit");
    append_record
        .validate()
        .expect("observe projection produces an audit append record accepted by the sink contract");
}

#[test]
fn pending_durable_audit_record_maps_admin_operation_families() {
    let admin = PendingDurableAuditRecord::new(
        2,
        admin_principal_binding(20, SurfaceScope::Administration, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            20,
            120,
            AdminOperation::InspectPlans,
            SurfaceScope::Administration,
            Permission::InspectPlans,
        ),
    )
    .expect("admin operation is durable-audit eligible");
    assert_eq!(
        admin.identity.family,
        DurableAuditEventFamily::AdminDecision
    );

    let hadr = PendingDurableAuditRecord::new(
        3,
        admin_principal_binding(21, SurfaceScope::Cluster, Permission::ClusterPromote),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            21,
            121,
            AdminOperation::ClusterPromote,
            SurfaceScope::Cluster,
            Permission::ClusterPromote,
        ),
    )
    .expect("cluster admin operation is durable-audit eligible");
    assert_eq!(hadr.identity.family, DurableAuditEventFamily::HadrDecision);
}

#[test]
fn pending_durable_audit_record_rejects_mismatched_observe_projection_evidence() {
    let err = PendingDurableAuditRecord::new(
        4,
        security_principal_binding(11, "user:other"),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(11, 90, "user:durable-audit"),
    )
    .expect_err("observe adapter rejects principal evidence that does not match the envelope");

    assert!(
        err.message()
            .contains("principal_id must match security audit trace")
    );
}

#[test]
fn pending_durable_audit_record_is_appendable_through_runtime_sink_trait() {
    let record = PendingDurableAuditRecord::new(
        5,
        security_principal_binding(12, "user:durable-audit"),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(12, 91, "user:durable-audit"),
    )
    .expect("observe adapter builds an appendable durable audit record");
    let expected_identity = record.identity;

    let mut sink = RecordingDurableAuditWalSink::default();
    let report = sink.append_durable_audit_record(record).expect(
        "runtime sink accepts the observe pending record via Into<DurableAuditAppendRecord>",
    );

    assert_eq!(report.identity, expected_identity);
    assert_eq!(report.evidence.record_lsn, 1);
    assert_eq!(sink.appended_event_kinds, vec!["SecurityAudit".to_string()]);
}

fn request_correlation(event_id: u128) -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        transaction_id: None,
        durable_lsn: None,
        protocol: None,
    }
}

fn security_envelope(event_id: u128, trace_id: u128, principal_id: &str) -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        format!("sha256:durable-audit-{event_id}"),
        "CN=durable-audit-contract",
        SurfaceScope::Application,
    )
    .expect("test certificate has explicit non-secret evidence");
    let principal = UserPrincipal::new(principal_id, UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = SecurityAuditTrace::new(
        TraceId::new(trace_id),
        SurfaceScope::Application,
        certificate,
        principal,
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Allowed,
        "permission grant recorded before dispatch",
    )
    .expect("security audit trace has explicit reason");

    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(event_id),
        TraceEvent::SecurityAudit(trace),
    )
    .expect("security audit envelope is valid")
}

fn security_principal_binding(event_id: u128, principal_id: &str) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: principal_id.to_string(),
        certificate_fingerprint: Some(format!("sha256:durable-audit-{event_id}")),
        surface: Some(SurfaceScope::Application),
        permission: Some(Permission::ExecuteProcedure),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
    }
}

fn admin_envelope(
    event_id: u128,
    trace_id: u128,
    operation: AdminOperation,
    surface: SurfaceScope,
    permission: Permission,
) -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        format!("sha256:admin-audit-{event_id}"),
        "CN=admin-audit-contract",
        surface,
    )
    .expect("test certificate has explicit non-secret evidence");
    let principal = UserPrincipal::new("user:admin-audit", UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = AdminOperationTrace::new(
        TraceId::new(trace_id),
        surface,
        certificate,
        principal,
        operation,
        permission,
        true,
        "admin decision recorded before visible side effect",
    )
    .expect("admin operation trace has explicit reason");

    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(event_id),
        TraceEvent::AdminOperation(trace),
    )
    .expect("admin operation envelope is valid")
}

fn admin_principal_binding(
    event_id: u128,
    surface: SurfaceScope,
    permission: Permission,
) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "user:admin-audit".to_string(),
        certificate_fingerprint: Some(format!("sha256:admin-audit-{event_id}")),
        surface: Some(surface),
        permission: Some(permission),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
    }
}

#[derive(Default)]
struct RecordingDurableAuditWalSink {
    appended_event_kinds: Vec<String>,
}

impl DurableAuditWalSink for RecordingDurableAuditWalSink {
    fn append_durable_audit_record<R>(
        &mut self,
        record: R,
    ) -> Result<DurableAuditSinkReport, DurableAuditSinkFailure>
    where
        R: Into<andromeda_audit::DurableAuditAppendRecord>,
    {
        let record = record.into();
        record.validate().map_err(|error| {
            DurableAuditSinkFailure::new(
                DurableAuditFailureKind::ValidationRejected,
                Some(record.identity),
                error.message().to_string(),
            )
            .expect("validation error text is valid audit sink failure evidence")
        })?;
        let next_lsn = self.appended_event_kinds.len() as u64 + 1;
        self.appended_event_kinds.push(record.event_kind);
        Ok(DurableAuditSinkReport {
            identity: record.identity,
            evidence: DurableAuditWalEvidence {
                record_lsn: next_lsn,
                durable_lsn: next_lsn,
                checksum: 0xA11D_1700 + next_lsn,
            },
            replay_behavior: record.replay_behavior,
            retention: record.retention,
        })
    }
}
