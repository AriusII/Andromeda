pub(crate) use andromeda_core::{RequestId, SessionId};
pub(crate) use andromeda_observe::{
    CertificateIdentity, DurableAuditEventFamily, DurableAuditPrincipalBinding,
    DurableAuditPruneBlockReason, DurableAuditReplayBehavior, DurableAuditReplayLsnRange,
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditReplayWindow,
    DurableAuditRetentionBoundary, DurableAuditRetentionManager, DurableAuditRetentionPolicy,
    DurableAuditSinkReport, DurableAuditWalSegmentArchiveProof, DurableAuditWalSink,
    EventCorrelation, EventEnvelope, EventId, FileDurableAuditWalSink, PendingDurableAuditRecord,
    Permission, SecurityAuditOutcome, SecurityAuditTrace, SecurityPolicyVersionEvidence,
    SurfaceScope, TraceEvent, TraceId, UserPrincipal, UserPrincipalKind,
};
pub(crate) use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn request_correlation(event_id: u128) -> EventCorrelation {
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

pub(crate) fn security_envelope(
    event_id: u128,
    trace_id: u128,
    principal_id: &str,
) -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        format!("sha256:durable-audit-retention-{event_id}"),
        "CN=durable-audit-retention",
        SurfaceScope::Administration,
    )
    .expect("test certificate has explicit non-secret evidence");
    let principal = UserPrincipal::new(principal_id, UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = SecurityAuditTrace::new(
        TraceId::new(trace_id),
        SurfaceScope::Administration,
        certificate,
        principal,
        Permission::InspectPlans,
        SecurityAuditOutcome::Allowed,
        format!("durable audit retention event {event_id}"),
    )
    .expect("security audit trace has explicit reason");

    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(event_id),
        TraceEvent::SecurityAudit(trace),
    )
    .expect("security audit envelope is valid")
}

pub(crate) fn principal_binding(
    event_id: u128,
    principal_id: &str,
) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: principal_id.to_string(),
        certificate_fingerprint: Some(format!("sha256:durable-audit-retention-{event_id}")),
        surface: Some(SurfaceScope::Administration),
        permission: Some(Permission::InspectPlans),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
    }
}

pub(crate) struct TempJournal {
    path: PathBuf,
}

impl TempJournal {
    pub(crate) fn new(test_name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after UNIX epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "andromeda-observe-{test_name}-{}-{nonce}.audit",
            std::process::id()
        ));
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempJournal {
    fn drop(&mut self) {
        for suffix in ["", ".chain", ".compact.tmp", ".lock"] {
            let mut path = self.path.as_os_str().to_os_string();
            path.push(suffix);
            let _ = fs::remove_file(PathBuf::from(path));
        }
    }
}

pub(crate) fn open_journal(test_name: &str) -> (TempJournal, FileDurableAuditWalSink) {
    let journal = TempJournal::new(test_name);
    let sink = FileDurableAuditWalSink::open(journal.path()).expect("journal opens");
    (journal, sink)
}

pub(crate) fn chain_hex_field(line: &str, label: &str) -> u64 {
    let marker = format!("|{label}=");
    let value = line
        .split(&marker)
        .nth(1)
        .and_then(|rest| rest.split('|').next())
        .expect("journal line carries requested chain field");
    u64::from_str_radix(value, 16).expect("chain field is fixed-width hex")
}

pub(crate) fn previous_chain_checksum(line: &str) -> u64 {
    chain_hex_field(line, "previous_chain_checksum")
}

pub(crate) fn chain_checksum(line: &str) -> u64 {
    chain_hex_field(line, "chain_checksum")
}

pub(crate) fn append_security_record(
    sink: &mut FileDurableAuditWalSink,
    event_id: u128,
    principal_id: &str,
    retention: DurableAuditRetentionBoundary,
) -> andromeda_observe::DurableAuditSinkReport {
    let record = PendingDurableAuditRecord::new(
        event_id as u64,
        principal_binding(event_id, principal_id),
        retention,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(event_id, event_id + 1_000, principal_id),
    )
    .expect("durable audit record contract is satisfied");

    sink.append_durable_audit_record(record)
        .expect("append is flushed before success")
}

pub(crate) fn replay_record_for_report(
    sink: &FileDurableAuditWalSink,
    report: DurableAuditSinkReport,
) -> DurableAuditReplayRecord {
    sink.replay(&replay_lsn_query(report.evidence.record_lsn))
        .expect("record can be replayed by durable LSN")
        .into_iter()
        .find(|record| record.report == report)
        .expect("appended report has matching replay record")
}

pub(crate) fn replay_lsn_query(record_lsn: u64) -> DurableAuditReplayQuery {
    DurableAuditReplayQuery {
        lsn_range: Some(DurableAuditReplayLsnRange::new(record_lsn, record_lsn)),
        ..DurableAuditReplayQuery::all()
    }
}

pub(crate) fn retention_manager_after_record_lsn(record_lsn: u64) -> DurableAuditRetentionManager {
    DurableAuditRetentionManager::new(
        DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(record_lsn + 1)
            .with_forensic_hold_preserved(false),
    )
    .expect("retention manager accepts explicit expiry policy")
}

pub(crate) fn archive_proof(
    report: DurableAuditSinkReport,
    archive_id: impl Into<String>,
) -> DurableAuditWalSegmentArchiveProof {
    archive_proof_with_checksum(report, archive_id, report.evidence.checksum)
}

pub(crate) fn archive_proof_with_checksum(
    report: DurableAuditSinkReport,
    archive_id: impl Into<String>,
    checksum: u64,
) -> DurableAuditWalSegmentArchiveProof {
    DurableAuditWalSegmentArchiveProof {
        archive_id: archive_id.into(),
        first_lsn: report.evidence.record_lsn,
        last_lsn: report.evidence.durable_lsn,
        checksum,
    }
}
