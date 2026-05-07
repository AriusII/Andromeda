use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::{
    TraceEventFamily, TraceQueryLsnRange, TraceQueryMetadata, TraceQueryPermissionMatrix,
    TraceQuerySpec,
};
use crate::{
    DurableAuditEventFamily, DurableAuditReplayBehavior, DurableAuditReplayRecord,
    DurableAuditRetentionBoundary, EventId, Permission, SurfaceScope, TraceId,
    events::{contains_sensitive_marker, redact_sensitive_evidence},
};

/// Administrative inspection adapter over replayed durable-audit WAL records.
///
/// Durable audit replay records intentionally do not carry the original
/// `EventEnvelope`, so this source returns durable-audit rows while preserving
/// the common trace inspection spec, metadata, bounds, ordering, and permission
/// gate. This is not an application execution surface.
#[derive(Debug, Clone, Copy)]
pub struct DurableAuditTraceQuerySource<'a> {
    records: &'a [DurableAuditReplayRecord],
}

impl<'a> DurableAuditTraceQuerySource<'a> {
    pub const fn new(records: &'a [DurableAuditReplayRecord]) -> Self {
        Self { records }
    }

    pub fn inspect(&self, spec: &TraceQuerySpec) -> AndromedaResult<DurableAuditTraceQueryResult> {
        spec.validate()?;
        validate_supported_filters(spec)?;

        let mut skipped = 0usize;
        let mut total_matching = 0usize;
        let mut rows = Vec::new();

        for record in self.records {
            record.validate()?;
            let row = DurableAuditTraceQueryRow::from_replay_record(record)?;
            if !row.matches(spec) {
                continue;
            }

            total_matching = total_matching.saturating_add(1);
            if skipped < spec.offset {
                skipped = skipped.saturating_add(1);
                continue;
            }
            if rows.len() < spec.limit {
                rows.push(row);
            }
        }

        let returned_rows = rows.len();
        Ok(DurableAuditTraceQueryResult {
            metadata: TraceQueryMetadata {
                limit: spec.limit,
                offset: spec.offset,
                returned_rows,
                total_matching_rows: spec.include_total_count.then_some(total_matching),
                truncated: total_matching.saturating_sub(spec.offset) > returned_rows,
                ordered_by_event_id_ascending: true,
                permission_matrix: TraceQueryPermissionMatrix::V1_ADMIN,
            },
            rows,
        })
    }

    pub fn query(&self, spec: &TraceQuerySpec) -> AndromedaResult<DurableAuditTraceQueryResult> {
        self.inspect(spec)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditTraceQueryResult {
    pub metadata: TraceQueryMetadata,
    pub rows: Vec<DurableAuditTraceQueryRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditTraceQueryRow {
    pub event_id: EventId,
    pub trace_id: TraceId,
    pub family: TraceEventFamily,
    pub durable_audit_family: DurableAuditEventFamily,
    pub sequence_number: u64,
    pub record_lsn: u64,
    pub durable_lsn: u64,
    pub checksum: u64,
    pub replay_behavior: DurableAuditReplayBehavior,
    pub retention: DurableAuditRetentionBoundary,
    pub principal_id: String,
    pub certificate_fingerprint: Option<String>,
    pub surface: Option<SurfaceScope>,
    pub permission: Option<Permission>,
    pub request_id: Option<andromeda_types::RequestId>,
    pub session_id: Option<andromeda_types::SessionId>,
    pub event_kind: String,
}

impl DurableAuditTraceQueryRow {
    pub fn from_replay_record(record: &DurableAuditReplayRecord) -> AndromedaResult<Self> {
        record.validate()?;
        let identity = record.report.identity;
        let evidence = record.report.evidence;
        Ok(Self {
            event_id: identity.event_id,
            trace_id: identity.trace_id,
            family: trace_family_for_replay_record(record)?,
            durable_audit_family: identity.family,
            sequence_number: identity.sequence_number,
            record_lsn: evidence.record_lsn,
            durable_lsn: evidence.durable_lsn,
            checksum: evidence.checksum,
            replay_behavior: record.report.replay_behavior,
            retention: record.report.retention,
            principal_id: redact_sensitive_evidence(&record.principal_binding.principal_id),
            certificate_fingerprint: record
                .principal_binding
                .certificate_fingerprint
                .as_deref()
                .map(redact_sensitive_evidence),
            surface: record.principal_binding.surface,
            permission: record.principal_binding.permission,
            request_id: record.principal_binding.request_id,
            session_id: record.principal_binding.session_id,
            event_kind: redact_sensitive_evidence(&record.event_kind),
        })
    }

    fn matches(&self, spec: &TraceQuerySpec) -> bool {
        let filter = &spec.filter;
        if let Some(trace_id) = filter.trace_id
            && self.trace_id != trace_id
        {
            return false;
        }
        if let Some(family) = filter.family
            && self.family != family
        {
            return false;
        }
        if let Some(range) = filter.lsn_range
            && !matches_lsn_range(self, range)
        {
            return false;
        }
        if let Some(principal) = &filter.principal
            && self.principal_id != *principal
        {
            return false;
        }
        true
    }
}

fn validate_supported_filters(spec: &TraceQuerySpec) -> AndromedaResult<()> {
    if let Some(principal) = &spec.filter.principal
        && contains_sensitive_marker(principal)
    {
        return Err(durable_inspection_error(
            "durable audit trace inspection principal filter must not contain secret evidence",
        ));
    }
    if spec.filter.catalog_version.is_some() {
        return Err(durable_inspection_error(
            "durable audit trace inspection cannot filter catalog_version because the durable audit journal does not carry catalog correlation",
        ));
    }
    if spec.filter.procedure_id.is_some() {
        return Err(durable_inspection_error(
            "durable audit trace inspection cannot filter procedure_id because the durable audit journal does not carry catalog object correlation",
        ));
    }
    Ok(())
}

fn matches_lsn_range(row: &DurableAuditTraceQueryRow, range: TraceQueryLsnRange) -> bool {
    range.contains(row.record_lsn) || range.contains(row.durable_lsn)
}

fn trace_family_for_replay_record(
    record: &DurableAuditReplayRecord,
) -> AndromedaResult<TraceEventFamily> {
    Ok(match record.report.identity.family {
        DurableAuditEventFamily::SecurityDecision => TraceEventFamily::SecurityAudit,
        DurableAuditEventFamily::AdminDecision
        | DurableAuditEventFamily::HadrDecision
        | DurableAuditEventFamily::BackupDecision
        | DurableAuditEventFamily::RestoreDecision
        | DurableAuditEventFamily::ForensicDecision
        | DurableAuditEventFamily::GenericAudit => TraceEventFamily::AdminAudit,
        DurableAuditEventFamily::AdmissionDecision => TraceEventFamily::Protocol,
        DurableAuditEventFamily::CatalogDecision => TraceEventFamily::ManifestCatalog,
        DurableAuditEventFamily::RecoveryDecision => recovery_trace_family(&record.event_kind)?,
    })
}

fn recovery_trace_family(event_kind: &str) -> AndromedaResult<TraceEventFamily> {
    match event_kind {
        "RecoveryStartup" => Ok(TraceEventFamily::Recovery),
        "WalAppend" | "WalFlush" | "CommitVisible" | "RollbackDurable" | "CorruptionBoundary" => {
            Ok(TraceEventFamily::Wal)
        }
        _ => Err(durable_inspection_error(
            "durable audit recovery replay record carries unknown event_kind evidence",
        )),
    }
}

fn durable_inspection_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}
