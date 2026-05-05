use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use andromeda_core::{AndromedaResult, RequestId, SessionId, digest::sha256};

use crate::TraceId;

use super::{
    EventEnvelope, EventId, Permission, SurfaceScope, TraceEvent, contains_sensitive_marker,
    observe_error,
};

/// Audit event family used by the durable audit WAL contract.
///
/// This is a classification boundary only. It does not route events through a
/// privileged meta-engine and intentionally has no "disabled" or "null" family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditEventFamily {
    SecurityDecision,
    AdminDecision,
    AdmissionDecision,
    CatalogDecision,
    RecoveryDecision,
    GenericAudit,
}

impl DurableAuditEventFamily {
    /// Families whose decisions can authorize security/admin/catalog visibility
    /// must have their audit record durable before the decision is exposed.
    pub const fn requires_wal_before_visible_decision(self) -> bool {
        matches!(
            self,
            Self::SecurityDecision | Self::AdminDecision | Self::CatalogDecision
        )
    }
}

/// Replay classification for a durable audit record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditReplayBehavior {
    /// Reconstruct forensic evidence only; replay must not re-authorize or
    /// re-execute the original operation.
    ForensicOnly,
    /// Rebuild an operator/security/catalog decision index from the committed
    /// record without applying business data effects.
    RebuildDecisionIndex,
    /// Mark a corruption boundary and stop normal replay at the associated LSN.
    CorruptionBoundary,
}

/// Retention boundary for durable audit evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditRetentionBoundary {
    /// Retain at least until the associated WAL segment is safely checkpointed
    /// and the external retention policy permits pruning.
    WalSegment,
    /// Retain through the catalog version retention window because the audit
    /// record explains catalog visibility.
    CatalogVersion,
    /// Retain through the configured security audit retention horizon.
    SecurityPolicy,
    /// Retain until a forensic hold is explicitly released.
    ForensicHold,
}

/// Typed failure behavior expected from a durable audit sink implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditFailureKind {
    ValidationRejected,
    WalAppendRejected,
    WalFlushRejected,
    CorruptionDetected,
    PermissionDenied,
    RetentionRejected,
}

impl DurableAuditFailureKind {
    /// Security/admin/catalog decisions must fail closed when their audit WAL
    /// record cannot be appended and flushed before the decision becomes
    /// externally visible.
    pub const fn requires_fail_closed(self) -> bool {
        match self {
            Self::ValidationRejected
            | Self::WalAppendRejected
            | Self::WalFlushRejected
            | Self::CorruptionDetected
            | Self::PermissionDenied
            | Self::RetentionRejected => true,
        }
    }
}

/// Typed failure report returned by durable audit sink implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditSinkFailure {
    pub kind: DurableAuditFailureKind,
    pub identity: Option<DurableAuditRecordIdentity>,
    pub reason: String,
}

impl DurableAuditSinkFailure {
    pub fn new(
        kind: DurableAuditFailureKind,
        identity: Option<DurableAuditRecordIdentity>,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let failure = Self {
            kind,
            identity,
            reason: reason.into(),
        };
        failure.validate()?;
        Ok(failure)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if let Some(identity) = self.identity {
            identity.validate()?;
        }
        if self.reason.trim().is_empty() {
            return Err(observe_error(
                "durable audit sink failure requires non-empty reason evidence",
            ));
        }
        if contains_sensitive_marker(&self.reason) {
            return Err(observe_error(
                "durable audit sink failure reason must not contain secret evidence",
            ));
        }
        Ok(())
    }

    pub fn requires_fail_closed(&self) -> bool {
        self.kind.requires_fail_closed()
    }
}

pub type DurableAuditSinkResult<T> = Result<T, DurableAuditSinkFailure>;

/// Stable identity for a durable audit record. The tuple `(event_id, trace_id,
/// family)` identifies the logical audit event; `sequence_number` provides a
/// per-sink ordering witness for replay reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DurableAuditRecordIdentity {
    pub event_id: EventId,
    pub trace_id: TraceId,
    pub family: DurableAuditEventFamily,
    pub sequence_number: u64,
}

impl DurableAuditRecordIdentity {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.event_id.is_zero() {
            return Err(observe_error(
                "durable audit record identity requires non-zero event_id",
            ));
        }
        if self.trace_id.is_zero() {
            return Err(observe_error(
                "durable audit record identity requires non-zero trace_id",
            ));
        }
        if self.sequence_number == 0 {
            return Err(observe_error(
                "durable audit record identity requires non-zero sequence_number",
            ));
        }
        Ok(())
    }
}

/// Principal/session binding copied into a durable audit record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditPrincipalBinding {
    pub principal_id: String,
    pub certificate_fingerprint: Option<String>,
    pub surface: Option<SurfaceScope>,
    pub permission: Option<Permission>,
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
}

impl DurableAuditPrincipalBinding {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.principal_id.trim().is_empty() {
            return Err(observe_error(
                "durable audit principal binding requires non-empty principal_id",
            ));
        }

        let certificate_is_empty = self
            .certificate_fingerprint
            .as_ref()
            .is_some_and(|fingerprint| fingerprint.trim().is_empty());
        if certificate_is_empty {
            return Err(observe_error(
                "durable audit principal binding certificate fingerprint must not be empty when present",
            ));
        }

        if contains_sensitive_marker(&self.principal_id)
            || self
                .certificate_fingerprint
                .as_ref()
                .is_some_and(|fingerprint| contains_sensitive_marker(fingerprint))
        {
            return Err(observe_error(
                "durable audit principal binding must not contain secret evidence",
            ));
        }

        if self
            .request_id
            .is_some_and(|request_id| request_id.get() == 0)
            || self
                .session_id
                .is_some_and(|session_id| session_id.get() == 0)
        {
            return Err(observe_error(
                "durable audit principal binding request/session ids must be non-zero when present",
            ));
        }

        Ok(())
    }
}

/// LSN and checksum evidence returned only after the audit record has crossed
/// the durable WAL boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditWalEvidence {
    pub record_lsn: u64,
    pub durable_lsn: u64,
    pub checksum: u64,
}

impl DurableAuditWalEvidence {
    pub const fn proves_durable(self) -> bool {
        self.record_lsn != 0 && self.durable_lsn >= self.record_lsn && self.checksum != 0
    }
}

/// Pending audit record. Implementations must WAL-append and flush this record
/// before reporting success for security/admin/catalog decisions that become
/// externally visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDurableAuditRecord {
    pub identity: DurableAuditRecordIdentity,
    pub principal_binding: DurableAuditPrincipalBinding,
    pub retention: DurableAuditRetentionBoundary,
    pub replay_behavior: DurableAuditReplayBehavior,
    pub envelope: EventEnvelope,
}

impl PendingDurableAuditRecord {
    pub fn new(
        sequence_number: u64,
        principal_binding: DurableAuditPrincipalBinding,
        retention: DurableAuditRetentionBoundary,
        replay_behavior: DurableAuditReplayBehavior,
        envelope: EventEnvelope,
    ) -> AndromedaResult<Self> {
        let family = durable_audit_family(&envelope.event).ok_or_else(|| {
            observe_error("event family is not eligible for durable audit WAL recording")
        })?;
        let record = Self {
            identity: DurableAuditRecordIdentity {
                event_id: envelope.event_id,
                trace_id: envelope.trace_id,
                family,
                sequence_number,
            },
            principal_binding,
            retention,
            replay_behavior,
            envelope,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.identity.validate()?;
        self.principal_binding.validate()?;
        self.envelope.validate()?;

        if self.identity.trace_id != self.envelope.trace_id {
            return Err(observe_error(
                "durable audit identity trace_id must match envelope trace_id",
            ));
        }
        if self.identity.event_id != self.envelope.event_id {
            return Err(observe_error(
                "durable audit identity event_id must match envelope event_id",
            ));
        }

        let Some(family) = durable_audit_family(&self.envelope.event) else {
            return Err(observe_error(
                "durable audit record requires an audit-eligible trace event",
            ));
        };
        if family != self.identity.family {
            return Err(observe_error(
                "durable audit identity family must match envelope event family",
            ));
        }

        if matches!(
            self.identity.family,
            DurableAuditEventFamily::SecurityDecision
        ) && !self.envelope.correlation.has_request_session()
        {
            return Err(observe_error(
                "durable security audit records require request/session correlation",
            ));
        }

        Ok(())
    }
}

/// Report returned by a durable audit sink after the WAL record is durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditSinkReport {
    pub identity: DurableAuditRecordIdentity,
    pub evidence: DurableAuditWalEvidence,
    pub replay_behavior: DurableAuditReplayBehavior,
    pub retention: DurableAuditRetentionBoundary,
}

impl DurableAuditSinkReport {
    pub fn validate(self) -> AndromedaResult<()> {
        self.identity.validate()?;
        if !self.evidence.proves_durable() {
            return Err(observe_error(
                "durable audit sink report requires non-zero WAL LSN/checksum evidence",
            ));
        }
        Ok(())
    }
}

/// Minimal trait boundary for a durable audit WAL sink.
///
/// Implementors must not provide a global audit-disable switch. Returning
/// success means the audit record has been appended to WAL, flushed through
/// `durable_lsn`, and can be found during forensic replay.
pub trait DurableAuditWalSink {
    fn append_durable_audit_record(
        &mut self,
        record: PendingDurableAuditRecord,
    ) -> DurableAuditSinkResult<DurableAuditSinkReport>;
}

/// Inclusive audit WAL LSN interval used by targeted replay queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditReplayLsnRange {
    pub start_lsn: u64,
    pub end_lsn: u64,
}

impl DurableAuditReplayLsnRange {
    pub const fn new(start_lsn: u64, end_lsn: u64) -> Self {
        Self { start_lsn, end_lsn }
    }

    pub const fn contains(self, lsn: u64) -> bool {
        self.start_lsn <= lsn && lsn <= self.end_lsn
    }

    pub const fn is_valid(self) -> bool {
        self.start_lsn != 0 && self.end_lsn != 0 && self.start_lsn <= self.end_lsn
    }
}

/// Closed replay filter for the durable audit journal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DurableAuditReplayQuery {
    pub family: Option<DurableAuditEventFamily>,
    pub trace_id: Option<TraceId>,
    pub principal_id: Option<String>,
    pub lsn_range: Option<DurableAuditReplayLsnRange>,
}

impl DurableAuditReplayQuery {
    pub fn all() -> Self {
        Self::default()
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if let Some(trace_id) = self.trace_id
            && trace_id.is_zero()
        {
            return Err(observe_error(
                "durable audit replay trace_id filter must be non-zero when present",
            ));
        }
        if let Some(principal_id) = &self.principal_id {
            if principal_id.trim().is_empty() {
                return Err(observe_error(
                    "durable audit replay principal filter must be non-empty when present",
                ));
            }
            if contains_sensitive_marker(principal_id) {
                return Err(observe_error(
                    "durable audit replay principal filter must not contain secret evidence",
                ));
            }
        }
        if let Some(range) = self.lsn_range
            && !range.is_valid()
        {
            return Err(observe_error(
                "durable audit replay LSN range must be non-zero and start_lsn <= end_lsn",
            ));
        }
        Ok(())
    }
}

/// Replayed durable audit index record.
///
/// This is intentionally narrower than [`PendingDurableAuditRecord`]: the
/// file-backed sink persists forensic identity, principal binding, retention,
/// replay behavior, and WAL evidence. It does not recreate an operation or
/// re-authorize the original request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditReplayRecord {
    pub report: DurableAuditSinkReport,
    pub principal_binding: DurableAuditPrincipalBinding,
    pub event_kind: String,
}

impl DurableAuditReplayRecord {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.report.validate()?;
        self.principal_binding.validate()?;
        if self.event_kind.trim().is_empty() {
            return Err(observe_error(
                "durable audit replay record requires event kind evidence",
            ));
        }
        if contains_sensitive_marker(&self.event_kind) {
            return Err(observe_error(
                "durable audit replay event kind must not contain secret evidence",
            ));
        }
        Ok(())
    }

    pub fn matches_query(&self, query: &DurableAuditReplayQuery) -> bool {
        if let Some(family) = query.family
            && self.report.identity.family != family
        {
            return false;
        }
        if let Some(trace_id) = query.trace_id
            && self.report.identity.trace_id != trace_id
        {
            return false;
        }
        if let Some(principal_id) = &query.principal_id
            && self.principal_binding.principal_id != *principal_id
        {
            return false;
        }
        if let Some(range) = query.lsn_range
            && !range.contains(self.report.evidence.record_lsn)
            && !range.contains(self.report.evidence.durable_lsn)
        {
            return false;
        }
        true
    }
}

/// Minimal append-only durable audit WAL sink backed by a local file.
///
/// The journal format is line-oriented and checksum-protected. Each append is
/// flushed with `sync_all` before success is reported, so the returned evidence
/// is available to a later replay after process restart. The sink stores only
/// audit index evidence and principal binding; it deliberately does not store
/// transaction ids or transaction durable LSNs from pre-transaction decisions.
#[derive(Debug, Clone)]
pub struct FileDurableAuditWalSink {
    path: PathBuf,
}

impl FileDurableAuditWalSink {
    pub fn open(path: impl AsRef<Path>) -> DurableAuditSinkResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                sink_failure(
                    DurableAuditFailureKind::WalAppendRejected,
                    None,
                    format!("failed to create durable audit journal directory: {error}"),
                )
            })?;
        }

        OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)
            .map_err(|error| {
                sink_failure(
                    DurableAuditFailureKind::WalAppendRejected,
                    None,
                    format!("failed to open durable audit journal: {error}"),
                )
            })?;

        let sink = Self { path };
        sink.replay(&DurableAuditReplayQuery::all())?;
        Ok(sink)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn replay(
        &self,
        query: &DurableAuditReplayQuery,
    ) -> DurableAuditSinkResult<Vec<DurableAuditReplayRecord>> {
        replay_durable_audit_journal(&self.path, query)
    }

    pub fn query(
        &self,
        query: &DurableAuditReplayQuery,
    ) -> DurableAuditSinkResult<Vec<DurableAuditReplayRecord>> {
        self.replay(query)
    }

    fn append_record(
        &self,
        record: PendingDurableAuditRecord,
    ) -> DurableAuditSinkResult<DurableAuditSinkReport> {
        let identity = record.identity;
        if let Err(error) = record.validate() {
            return Err(sink_failure(
                DurableAuditFailureKind::ValidationRejected,
                Some(identity),
                error.message().to_string(),
            ));
        }

        let record_lsn = next_record_lsn(&self.path, Some(identity))?;
        let event_kind = format!("{:?}", record.envelope.event.kind());
        let mut replay_record = DurableAuditReplayRecord {
            report: DurableAuditSinkReport {
                identity,
                evidence: DurableAuditWalEvidence {
                    record_lsn,
                    durable_lsn: record_lsn,
                    checksum: 1,
                },
                replay_behavior: record.replay_behavior,
                retention: record.retention,
            },
            principal_binding: record.principal_binding,
            event_kind,
        };
        replay_record.report.evidence.checksum = checksum64(
            journal_payload(&replay_record)
                .map_err(|error| {
                    sink_failure(
                        DurableAuditFailureKind::ValidationRejected,
                        Some(identity),
                        error.message().to_string(),
                    )
                })?
                .as_bytes(),
        );
        replay_record.validate().map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::ValidationRejected,
                Some(identity),
                error.message().to_string(),
            )
        })?;

        let line = journal_line(&replay_record).map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::ValidationRejected,
                Some(identity),
                error.message().to_string(),
            )
        })?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| {
                sink_failure(
                    DurableAuditFailureKind::WalAppendRejected,
                    Some(identity),
                    format!("failed to append durable audit journal record: {error}"),
                )
            })?;
        file.write_all(line.as_bytes()).map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::WalAppendRejected,
                Some(identity),
                format!("failed to write durable audit journal record: {error}"),
            )
        })?;
        file.sync_all().map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::WalFlushRejected,
                Some(identity),
                format!("failed to flush durable audit journal record: {error}"),
            )
        })?;

        Ok(replay_record.report)
    }
}

impl DurableAuditWalSink for FileDurableAuditWalSink {
    fn append_durable_audit_record(
        &mut self,
        record: PendingDurableAuditRecord,
    ) -> DurableAuditSinkResult<DurableAuditSinkReport> {
        self.append_record(record)
    }
}

pub fn durable_audit_family(event: &TraceEvent) -> Option<DurableAuditEventFamily> {
    match event {
        TraceEvent::SecurityAudit(_) | TraceEvent::AuthorizationDenied(_) => {
            Some(DurableAuditEventFamily::SecurityDecision)
        }
        TraceEvent::AdminOperation(_) => Some(DurableAuditEventFamily::AdminDecision),
        TraceEvent::ContractRejected(_) => Some(DurableAuditEventFamily::AdmissionDecision),
        TraceEvent::CatalogMutation(_) | TraceEvent::Manifest(_) => {
            Some(DurableAuditEventFamily::CatalogDecision)
        }
        TraceEvent::Wal(_)
        | TraceEvent::WalEvent(_)
        | TraceEvent::CommitVisible(_)
        | TraceEvent::RollbackDurable(_)
        | TraceEvent::RecoveryStartup(_)
        | TraceEvent::CorruptionBoundary(_) => Some(DurableAuditEventFamily::RecoveryDecision),
        TraceEvent::Audit(_) => Some(DurableAuditEventFamily::GenericAudit),
        _ => None,
    }
}

const JOURNAL_PREFIX: &str = "andromeda-durable-audit-v1";
const NONE_FIELD: &str = "-";

fn replay_durable_audit_journal(
    path: &Path,
    query: &DurableAuditReplayQuery,
) -> DurableAuditSinkResult<Vec<DurableAuditReplayRecord>> {
    query.validate().map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::ValidationRejected,
            None,
            error.message().to_string(),
        )
    })?;

    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(path).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to read durable audit journal: {error}"),
        )
    })?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    let mut previous_lsn = 0u64;

    for (line_index, line) in reader.lines().enumerate() {
        let line = line.map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                format!("failed to read durable audit journal line: {error}"),
            )
        })?;
        let record = parse_journal_line(&line).map_err(|reason| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                format!(
                    "durable audit journal corruption at line {}: {reason}",
                    line_index + 1
                ),
            )
        })?;
        if record.report.evidence.record_lsn <= previous_lsn {
            return Err(sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                Some(record.report.identity),
                "durable audit journal record LSNs must increase strictly",
            ));
        }
        previous_lsn = record.report.evidence.record_lsn;

        if record.matches_query(query) {
            records.push(record);
        }
    }

    Ok(records)
}

fn next_record_lsn(
    path: &Path,
    identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<u64> {
    let len = fs::metadata(path)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::WalAppendRejected,
                identity,
                format!("failed to inspect durable audit journal length: {error}"),
            )
        })?
        .len();
    Ok(len.saturating_add(1))
}

fn journal_line(record: &DurableAuditReplayRecord) -> AndromedaResult<String> {
    let payload = journal_payload(record)?;
    let checksum = checksum64(payload.as_bytes());
    if checksum != record.report.evidence.checksum {
        return Err(observe_error(
            "durable audit journal checksum must match report evidence",
        ));
    }
    Ok(format!("{payload}|checksum={checksum:016x}\n"))
}

fn journal_payload(record: &DurableAuditReplayRecord) -> AndromedaResult<String> {
    record.validate()?;
    let report = record.report;
    let binding = &record.principal_binding;
    Ok(format!(
        "{JOURNAL_PREFIX}|record_lsn={}|durable_lsn={}|event_id={}|trace_id={}|family={}|sequence={}|retention={}|replay={}|principal_id={}|certificate_fingerprint={}|surface={}|permission={}|request_id={}|session_id={}|event_kind={}",
        report.evidence.record_lsn,
        report.evidence.durable_lsn,
        report.identity.event_id.get(),
        report.identity.trace_id.get(),
        format_family(report.identity.family),
        report.identity.sequence_number,
        format_retention(report.retention),
        format_replay(report.replay_behavior),
        encode_string(&binding.principal_id),
        encode_optional_string(binding.certificate_fingerprint.as_deref()),
        binding.surface.map(format_surface).unwrap_or(NONE_FIELD),
        binding
            .permission
            .map(format_permission)
            .unwrap_or(NONE_FIELD),
        binding
            .request_id
            .map(|request_id| request_id.get().to_string())
            .unwrap_or_else(|| NONE_FIELD.to_string()),
        binding
            .session_id
            .map(|session_id| session_id.get().to_string())
            .unwrap_or_else(|| NONE_FIELD.to_string()),
        encode_string(&record.event_kind),
    ))
}

fn parse_journal_line(line: &str) -> Result<DurableAuditReplayRecord, String> {
    let (payload, checksum_field) = line
        .rsplit_once("|checksum=")
        .ok_or_else(|| "missing checksum field".to_string())?;
    let expected_checksum = u64::from_str_radix(checksum_field, 16)
        .map_err(|_| "checksum field is not valid hex".to_string())?;
    let actual_checksum = checksum64(payload.as_bytes());
    if actual_checksum != expected_checksum {
        return Err("checksum mismatch".to_string());
    }

    let fields: Vec<&str> = payload.split('|').collect();
    if fields.len() != 16 {
        return Err("unexpected durable audit journal field count".to_string());
    }
    if fields[0] != JOURNAL_PREFIX {
        return Err("unexpected durable audit journal prefix".to_string());
    }

    let record_lsn = parse_u64_field(fields[1], "record_lsn")?;
    let durable_lsn = parse_u64_field(fields[2], "durable_lsn")?;
    let event_id = parse_u128_field(fields[3], "event_id")?;
    let trace_id = parse_u128_field(fields[4], "trace_id")?;
    let family = parse_family(strip_field(fields[5], "family")?)?;
    let sequence_number = parse_u64_field(fields[6], "sequence")?;
    let retention = parse_retention(strip_field(fields[7], "retention")?)?;
    let replay_behavior = parse_replay(strip_field(fields[8], "replay")?)?;
    let principal_id = decode_string(strip_field(fields[9], "principal_id")?)?;
    let certificate_fingerprint =
        decode_optional_string(strip_field(fields[10], "certificate_fingerprint")?)?;
    let surface = parse_optional_surface(strip_field(fields[11], "surface")?)?;
    let permission = parse_optional_permission(strip_field(fields[12], "permission")?)?;
    let request_id =
        parse_optional_u64(strip_field(fields[13], "request_id")?)?.map(RequestId::new);
    let session_id =
        parse_optional_u64(strip_field(fields[14], "session_id")?)?.map(SessionId::new);
    let event_kind = decode_string(strip_field(fields[15], "event_kind")?)?;

    let record = DurableAuditReplayRecord {
        report: DurableAuditSinkReport {
            identity: DurableAuditRecordIdentity {
                event_id: EventId::new(event_id),
                trace_id: TraceId::new(trace_id),
                family,
                sequence_number,
            },
            evidence: DurableAuditWalEvidence {
                record_lsn,
                durable_lsn,
                checksum: expected_checksum,
            },
            replay_behavior,
            retention,
        },
        principal_binding: DurableAuditPrincipalBinding {
            principal_id,
            certificate_fingerprint,
            surface,
            permission,
            request_id,
            session_id,
        },
        event_kind,
    };
    record
        .validate()
        .map_err(|error| error.message().to_string())?;
    Ok(record)
}

fn parse_u64_field(field: &str, label: &str) -> Result<u64, String> {
    strip_field(field, label)?
        .parse::<u64>()
        .map_err(|_| format!("{label} field is not a valid u64"))
}

fn parse_u128_field(field: &str, label: &str) -> Result<u128, String> {
    strip_field(field, label)?
        .parse::<u128>()
        .map_err(|_| format!("{label} field is not a valid u128"))
}

fn parse_optional_u64(value: &str) -> Result<Option<u64>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|_| "optional integer field is not valid u64".to_string())
}

fn strip_field<'a>(field: &'a str, label: &str) -> Result<&'a str, String> {
    field
        .strip_prefix(label)
        .and_then(|rest| rest.strip_prefix('='))
        .ok_or_else(|| format!("missing {label} field"))
}

fn checksum64(bytes: &[u8]) -> u64 {
    let digest = sha256(bytes);
    let checksum = u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ]);
    checksum.max(1)
}

fn encode_string(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn encode_optional_string(value: Option<&str>) -> String {
    value
        .map(encode_string)
        .unwrap_or_else(|| NONE_FIELD.to_string())
}

fn decode_string(value: &str) -> Result<String, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex string has odd length".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for offset in (0..value.len()).step_by(2) {
        let byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| "hex string contains non-hex bytes".to_string())?;
        bytes.push(byte);
    }
    String::from_utf8(bytes).map_err(|_| "hex string is not valid UTF-8".to_string())
}

fn decode_optional_string(value: &str) -> Result<Option<String>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    decode_string(value).map(Some)
}

fn format_family(value: DurableAuditEventFamily) -> &'static str {
    match value {
        DurableAuditEventFamily::SecurityDecision => "SecurityDecision",
        DurableAuditEventFamily::AdminDecision => "AdminDecision",
        DurableAuditEventFamily::AdmissionDecision => "AdmissionDecision",
        DurableAuditEventFamily::CatalogDecision => "CatalogDecision",
        DurableAuditEventFamily::RecoveryDecision => "RecoveryDecision",
        DurableAuditEventFamily::GenericAudit => "GenericAudit",
    }
}

fn parse_family(value: &str) -> Result<DurableAuditEventFamily, String> {
    match value {
        "SecurityDecision" => Ok(DurableAuditEventFamily::SecurityDecision),
        "AdminDecision" => Ok(DurableAuditEventFamily::AdminDecision),
        "AdmissionDecision" => Ok(DurableAuditEventFamily::AdmissionDecision),
        "CatalogDecision" => Ok(DurableAuditEventFamily::CatalogDecision),
        "RecoveryDecision" => Ok(DurableAuditEventFamily::RecoveryDecision),
        "GenericAudit" => Ok(DurableAuditEventFamily::GenericAudit),
        _ => Err("unknown durable audit family".to_string()),
    }
}

fn format_replay(value: DurableAuditReplayBehavior) -> &'static str {
    match value {
        DurableAuditReplayBehavior::ForensicOnly => "ForensicOnly",
        DurableAuditReplayBehavior::RebuildDecisionIndex => "RebuildDecisionIndex",
        DurableAuditReplayBehavior::CorruptionBoundary => "CorruptionBoundary",
    }
}

fn parse_replay(value: &str) -> Result<DurableAuditReplayBehavior, String> {
    match value {
        "ForensicOnly" => Ok(DurableAuditReplayBehavior::ForensicOnly),
        "RebuildDecisionIndex" => Ok(DurableAuditReplayBehavior::RebuildDecisionIndex),
        "CorruptionBoundary" => Ok(DurableAuditReplayBehavior::CorruptionBoundary),
        _ => Err("unknown durable audit replay behavior".to_string()),
    }
}

fn format_retention(value: DurableAuditRetentionBoundary) -> &'static str {
    match value {
        DurableAuditRetentionBoundary::WalSegment => "WalSegment",
        DurableAuditRetentionBoundary::CatalogVersion => "CatalogVersion",
        DurableAuditRetentionBoundary::SecurityPolicy => "SecurityPolicy",
        DurableAuditRetentionBoundary::ForensicHold => "ForensicHold",
    }
}

fn parse_retention(value: &str) -> Result<DurableAuditRetentionBoundary, String> {
    match value {
        "WalSegment" => Ok(DurableAuditRetentionBoundary::WalSegment),
        "CatalogVersion" => Ok(DurableAuditRetentionBoundary::CatalogVersion),
        "SecurityPolicy" => Ok(DurableAuditRetentionBoundary::SecurityPolicy),
        "ForensicHold" => Ok(DurableAuditRetentionBoundary::ForensicHold),
        _ => Err("unknown durable audit retention boundary".to_string()),
    }
}

fn format_surface(value: SurfaceScope) -> &'static str {
    match value {
        SurfaceScope::Application => "Application",
        SurfaceScope::Administration => "Administration",
        SurfaceScope::Cluster => "Cluster",
        SurfaceScope::BackupAgent => "BackupAgent",
        SurfaceScope::MonitoringAgent => "MonitoringAgent",
    }
}

fn parse_optional_surface(value: &str) -> Result<Option<SurfaceScope>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    match value {
        "Application" => Ok(Some(SurfaceScope::Application)),
        "Administration" => Ok(Some(SurfaceScope::Administration)),
        "Cluster" => Ok(Some(SurfaceScope::Cluster)),
        "BackupAgent" => Ok(Some(SurfaceScope::BackupAgent)),
        "MonitoringAgent" => Ok(Some(SurfaceScope::MonitoringAgent)),
        _ => Err("unknown durable audit surface scope".to_string()),
    }
}

fn format_permission(value: Permission) -> &'static str {
    match value {
        Permission::ExecuteProcedure => "ExecuteProcedure",
        Permission::ReadContract => "ReadContract",
        Permission::CreateTable => "CreateTable",
        Permission::CreateMap => "CreateMap",
        Permission::CreateProcedure => "CreateProcedure",
        Permission::ImportDefinitionBatch => "ImportDefinitionBatch",
        Permission::DebugProcedure => "DebugProcedure",
        Permission::ReadProcedureStore => "ReadProcedureStore",
        Permission::InspectPlans => "InspectPlans",
        Permission::ManageSecurity => "ManageSecurity",
        Permission::RotateCertificate => "RotateCertificate",
        Permission::RevokeCertificateIdentity => "RevokeCertificateIdentity",
        Permission::Backup => "Backup",
        Permission::Restore => "Restore",
        Permission::ForensicStart => "ForensicStart",
        Permission::ClusterPromote => "ClusterPromote",
        Permission::FenceNode => "FenceNode",
        Permission::UpdateClusterManifest => "UpdateClusterManifest",
    }
}

fn parse_optional_permission(value: &str) -> Result<Option<Permission>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    match value {
        "ExecuteProcedure" => Ok(Some(Permission::ExecuteProcedure)),
        "ReadContract" => Ok(Some(Permission::ReadContract)),
        "CreateTable" => Ok(Some(Permission::CreateTable)),
        "CreateMap" => Ok(Some(Permission::CreateMap)),
        "CreateProcedure" => Ok(Some(Permission::CreateProcedure)),
        "ImportDefinitionBatch" => Ok(Some(Permission::ImportDefinitionBatch)),
        "DebugProcedure" => Ok(Some(Permission::DebugProcedure)),
        "ReadProcedureStore" => Ok(Some(Permission::ReadProcedureStore)),
        "InspectPlans" => Ok(Some(Permission::InspectPlans)),
        "ManageSecurity" => Ok(Some(Permission::ManageSecurity)),
        "RotateCertificate" => Ok(Some(Permission::RotateCertificate)),
        "RevokeCertificateIdentity" => Ok(Some(Permission::RevokeCertificateIdentity)),
        "Backup" => Ok(Some(Permission::Backup)),
        "Restore" => Ok(Some(Permission::Restore)),
        "ForensicStart" => Ok(Some(Permission::ForensicStart)),
        "ClusterPromote" => Ok(Some(Permission::ClusterPromote)),
        "FenceNode" => Ok(Some(Permission::FenceNode)),
        "UpdateClusterManifest" => Ok(Some(Permission::UpdateClusterManifest)),
        _ => Err("unknown durable audit permission".to_string()),
    }
}

fn sink_failure(
    kind: DurableAuditFailureKind,
    identity: Option<DurableAuditRecordIdentity>,
    reason: impl Into<String>,
) -> DurableAuditSinkFailure {
    let identity = identity.filter(|identity| identity.validate().is_ok());
    let reason = reason.into();
    let reason = if reason.trim().is_empty() || contains_sensitive_marker(&reason) {
        "durable audit sink operation failed".to_string()
    } else {
        reason
    };

    DurableAuditSinkFailure::new(kind, identity, reason).unwrap_or(DurableAuditSinkFailure {
        kind,
        identity: None,
        reason: "durable audit sink operation failed".to_string(),
    })
}
