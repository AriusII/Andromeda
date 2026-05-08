use andromeda_core::digest::sha256;
pub(crate) use andromeda_core::{CatalogObjectId, CatalogVersion, RequestId, SessionId};
pub(crate) use andromeda_observe::{
    AdminOperation, AdminOperationTrace, CatalogMutationTrace, CertificateIdentity,
    DurableAuditDecisionGate, DurableAuditEventFamily, DurableAuditFailureKind,
    DurableAuditPrincipalBinding, DurableAuditReplayBehavior, DurableAuditReplayLsnRange,
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditRetentionBoundary,
    DurableAuditSinkFailure, DurableAuditSinkReport, DurableAuditWalEvidence, DurableAuditWalSink,
    EventCorrelation, EventEnvelope, EventId, FileDurableAuditWalSink, PendingDurableAuditRecord,
    Permission, SecurityAuditOutcome, SecurityAuditTrace, SecurityPolicyVersionEvidence,
    SurfaceScope, TraceEvent, TraceId, UserPrincipal, UserPrincipalKind,
};
pub(crate) use std::fs;
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn request_correlation() -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(7)),
        session_id: Some(SessionId::new(8)),
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        transaction_id: None,
        durable_lsn: None,
        protocol: None,
    }
}

pub(crate) fn security_envelope() -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        "sha256:durable-audit-contract",
        "CN=durable-audit-contract",
        SurfaceScope::Application,
    )
    .expect("test certificate has explicit non-secret evidence");
    let principal = UserPrincipal::new("user:durable-audit", UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = SecurityAuditTrace::new(
        TraceId::new(90),
        SurfaceScope::Application,
        certificate,
        principal,
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Allowed,
        "permission grant recorded before dispatch",
    )
    .expect("security audit trace has explicit reason");

    EventEnvelope::new(
        EventId::new(11),
        request_correlation(),
        TraceEvent::SecurityAudit(trace),
    )
    .expect("security audit envelope is valid")
}

pub(crate) fn principal_binding() -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "user:durable-audit".to_string(),
        certificate_fingerprint: Some("sha256:durable-audit-contract".to_string()),
        surface: Some(SurfaceScope::Application),
        permission: Some(Permission::ExecuteProcedure),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(7)),
        session_id: Some(SessionId::new(8)),
    }
}

pub(crate) fn admin_envelope(
    event_id: u128,
    trace_id: u128,
    operation: AdminOperation,
    permission: Permission,
) -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        format!("sha256:admin-audit-{event_id}"),
        "CN=admin-audit-contract",
        SurfaceScope::Administration,
    )
    .expect("test certificate has explicit non-secret evidence");
    let principal = UserPrincipal::new("user:admin-audit", UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = AdminOperationTrace::new(
        TraceId::new(trace_id),
        SurfaceScope::Administration,
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
        request_correlation(),
        TraceEvent::AdminOperation(trace),
    )
    .expect("admin operation envelope is valid")
}

pub(crate) fn admin_principal_binding(
    event_id: u128,
    permission: Permission,
) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "user:admin-audit".to_string(),
        certificate_fingerprint: Some(format!("sha256:admin-audit-{event_id}")),
        surface: Some(SurfaceScope::Administration),
        permission: Some(permission),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(7)),
        session_id: Some(SessionId::new(8)),
    }
}

pub(crate) fn hadr_envelope(
    event_id: u128,
    trace_id: u128,
    operation: AdminOperation,
    permission: Permission,
) -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        format!("sha256:hadr-audit-{event_id}"),
        "CN=hadr-audit-contract",
        SurfaceScope::Cluster,
    )
    .expect("test certificate has explicit cluster evidence");
    let principal = UserPrincipal::new("svc:hadr-audit", UserPrincipalKind::Service)
        .expect("test service principal has explicit evidence");
    let trace = AdminOperationTrace::new(
        TraceId::new(trace_id),
        SurfaceScope::Cluster,
        certificate,
        principal,
        operation,
        permission,
        true,
        "cluster decision recorded before primary visibility",
    )
    .expect("HADR operation trace has explicit reason");

    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(),
        TraceEvent::AdminOperation(trace),
    )
    .expect("HADR operation envelope is valid")
}

pub(crate) fn hadr_principal_binding(
    event_id: u128,
    permission: Permission,
) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "svc:hadr-audit".to_string(),
        certificate_fingerprint: Some(format!("sha256:hadr-audit-{event_id}")),
        surface: Some(SurfaceScope::Cluster),
        permission: Some(permission),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(7)),
        session_id: Some(SessionId::new(8)),
    }
}

pub(crate) fn catalog_publication_envelope() -> EventEnvelope {
    let catalog_version = CatalogVersion::new(42);
    let object_id = CatalogObjectId::new(700);
    let trace = CatalogMutationTrace {
        trace_id: TraceId::new(142),
        catalog_version,
        object_id: Some(object_id),
        action: "publication_manifest_committed".to_string(),
    };
    let correlation = EventCorrelation {
        request_id: Some(RequestId::new(70)),
        session_id: Some(SessionId::new(80)),
        contract_hash: None,
        catalog_version: Some(catalog_version),
        catalog_object_id: Some(object_id),
        transaction_id: None,
        durable_lsn: None,
        protocol: None,
    };

    EventEnvelope::new(
        EventId::new(142),
        correlation,
        TraceEvent::CatalogMutation(trace),
    )
    .expect("catalog publication envelope is valid")
}

pub(crate) fn catalog_principal_binding() -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "svc:catalog-publisher".to_string(),
        certificate_fingerprint: Some("sha256:catalog-publication".to_string()),
        surface: Some(SurfaceScope::Administration),
        permission: Some(Permission::ImportDefinitionBatch),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(70)),
        session_id: Some(SessionId::new(80)),
    }
}

pub(crate) fn temp_journal_path(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after UNIX epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "andromeda-observe-{test_name}-{}-{nonce}.audit",
        std::process::id()
    ))
}

pub(crate) fn hex_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

pub(crate) fn durable_audit_test_checksum64(bytes: &[u8]) -> u64 {
    let digest = sha256(bytes);
    let checksum = u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ]);
    checksum.max(1)
}

pub(crate) fn journal_line_with_checksum(payload: &str) -> String {
    let checksum = durable_audit_test_checksum64(payload.as_bytes());
    format!("{payload}|checksum={checksum:016x}\n")
}

pub(crate) fn journal_line_with_chain(payload: &str, previous_chain_checksum: u64) -> String {
    let checksum = durable_audit_test_checksum64(payload.as_bytes());
    let chain_checksum = durable_audit_test_checksum64(
        format!("{previous_chain_checksum:016x}|{checksum:016x}|{payload}").as_bytes(),
    );
    format!(
        "{payload}|previous_chain_checksum={previous_chain_checksum:016x}|chain_checksum={chain_checksum:016x}|checksum={checksum:016x}\n"
    )
}

pub(crate) fn journal_payload_from_line(line: &str) -> &str {
    line.rsplit_once("|previous_chain_checksum=")
        .expect("journal line carries chain predecessor evidence")
        .0
}

pub(crate) fn chain_checksum_from_line(line: &str) -> u64 {
    let (_, chain_and_checksum) = line
        .rsplit_once("|chain_checksum=")
        .expect("journal line carries chain checksum evidence");
    let (chain_checksum, _) = chain_and_checksum
        .split_once("|checksum=")
        .expect("journal line carries trailing record checksum");
    u64::from_str_radix(chain_checksum, 16).expect("chain checksum is fixed-width hex")
}

pub(crate) fn journal_chain_anchor_path(path: &Path) -> PathBuf {
    let mut anchor = path.as_os_str().to_os_string();
    anchor.push(".chain");
    PathBuf::from(anchor)
}

pub(crate) struct FailingDurableAuditWalSink;

impl DurableAuditWalSink for FailingDurableAuditWalSink {
    fn append_durable_audit_record(
        &mut self,
        record: PendingDurableAuditRecord,
    ) -> Result<DurableAuditSinkReport, DurableAuditSinkFailure> {
        Err(DurableAuditSinkFailure::new(
            DurableAuditFailureKind::WalFlushRejected,
            Some(record.identity),
            "simulated durable audit WAL flush failure",
        )
        .expect("test failure reason is valid"))
    }
}
