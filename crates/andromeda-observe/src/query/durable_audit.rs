use andromeda_audit as audit;
use andromeda_error::AndromedaResult;

use super::{
    TraceEventFamily, TraceQueryLsnRange, TraceQueryMetadata, TraceQueryPermissionMatrix,
    TraceQuerySpec,
};
use crate::{
    DurableAuditEventFamily, DurableAuditReplayBehavior, DurableAuditReplayRecord,
    DurableAuditRetentionBoundary, EventId, Permission, SurfaceScope, TraceId,
};

/// Compatibility adapter over the audit-owned durable replay query source.
#[derive(Debug, Clone, Copy)]
pub struct DurableAuditTraceQuerySource<'a> {
    inner: audit::DurableAuditTraceQuerySource<'a>,
}

impl<'a> DurableAuditTraceQuerySource<'a> {
    pub const fn new(records: &'a [DurableAuditReplayRecord]) -> Self {
        Self {
            inner: audit::DurableAuditTraceQuerySource::new(records),
        }
    }

    pub fn inspect(&self, spec: &TraceQuerySpec) -> AndromedaResult<DurableAuditTraceQueryResult> {
        let result = self.inner.inspect(&audit_query_spec(spec))?;
        Ok(DurableAuditTraceQueryResult {
            metadata: trace_query_metadata(result.metadata),
            rows: result
                .rows
                .into_iter()
                .map(DurableAuditTraceQueryRow::from_audit_row)
                .collect::<AndromedaResult<Vec<_>>>()?,
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
        audit::DurableAuditTraceQueryRow::from_replay_record(record).and_then(Self::from_audit_row)
    }

    fn from_audit_row(row: audit::DurableAuditTraceQueryRow) -> AndromedaResult<Self> {
        Ok(Self {
            event_id: row.event_id,
            trace_id: row.trace_id,
            family: trace_event_family(row.family),
            durable_audit_family: row.durable_audit_family,
            sequence_number: row.sequence_number,
            record_lsn: row.record_lsn,
            durable_lsn: row.durable_lsn,
            checksum: row.checksum,
            replay_behavior: row.replay_behavior,
            retention: row.retention,
            principal_id: row.principal_id,
            certificate_fingerprint: row.certificate_fingerprint,
            surface: row.surface,
            permission: row.permission,
            request_id: row.request_id,
            session_id: row.session_id,
            event_kind: row.event_kind,
        })
    }
}

fn audit_query_spec(spec: &TraceQuerySpec) -> audit::DurableAuditTraceQuerySpec {
    audit::DurableAuditTraceQuerySpec {
        filter: audit::DurableAuditTraceQueryFilter {
            trace_id: spec.filter.trace_id,
            family: spec.filter.family.map(audit_trace_family),
            lsn_range: spec.filter.lsn_range.map(audit_lsn_range),
            catalog_version: spec.filter.catalog_version,
            procedure_id: spec.filter.procedure_id,
            principal: spec.filter.principal.clone(),
        },
        limit: spec.limit,
        offset: spec.offset,
        include_total_count: spec.include_total_count,
    }
}

fn audit_lsn_range(range: TraceQueryLsnRange) -> audit::DurableAuditTraceQueryLsnRange {
    audit::DurableAuditTraceQueryLsnRange::new(range.start_lsn, range.end_lsn)
}

fn trace_query_metadata(metadata: audit::DurableAuditTraceQueryMetadata) -> TraceQueryMetadata {
    TraceQueryMetadata {
        limit: metadata.limit,
        offset: metadata.offset,
        returned_rows: metadata.returned_rows,
        total_matching_rows: metadata.total_matching_rows,
        truncated: metadata.truncated,
        ordered_by_event_id_ascending: metadata.ordered_by_event_id_ascending,
        permission_matrix: trace_query_permission_matrix(metadata.permission_matrix),
    }
}

fn trace_query_permission_matrix(
    matrix: audit::DurableAuditTraceQueryPermissionMatrix,
) -> TraceQueryPermissionMatrix {
    TraceQueryPermissionMatrix {
        surface: matrix.surface,
        required_permission: matrix.required_permission,
        audit_operation: matrix.audit_operation,
        audit_required: matrix.audit_required,
    }
}

fn audit_trace_family(family: TraceEventFamily) -> audit::DurableAuditTraceFamily {
    match family {
        TraceEventFamily::Decision => audit::DurableAuditTraceFamily::Decision,
        TraceEventFamily::ProcedureInvocation => {
            audit::DurableAuditTraceFamily::ProcedureInvocation
        },
        TraceEventFamily::Wal => audit::DurableAuditTraceFamily::Wal,
        TraceEventFamily::Recovery => audit::DurableAuditTraceFamily::Recovery,
        TraceEventFamily::ManifestCatalog => audit::DurableAuditTraceFamily::ManifestCatalog,
        TraceEventFamily::Protocol => audit::DurableAuditTraceFamily::Protocol,
        TraceEventFamily::SecurityAudit => audit::DurableAuditTraceFamily::SecurityAudit,
        TraceEventFamily::AdminAudit => audit::DurableAuditTraceFamily::AdminAudit,
        TraceEventFamily::Resource => audit::DurableAuditTraceFamily::Resource,
        TraceEventFamily::Io => audit::DurableAuditTraceFamily::Io,
        TraceEventFamily::Gpu => audit::DurableAuditTraceFamily::Gpu,
        TraceEventFamily::Transaction => audit::DurableAuditTraceFamily::Transaction,
    }
}

fn trace_event_family(family: audit::DurableAuditTraceFamily) -> TraceEventFamily {
    match family {
        audit::DurableAuditTraceFamily::Decision => TraceEventFamily::Decision,
        audit::DurableAuditTraceFamily::ProcedureInvocation => {
            TraceEventFamily::ProcedureInvocation
        },
        audit::DurableAuditTraceFamily::Wal => TraceEventFamily::Wal,
        audit::DurableAuditTraceFamily::Recovery => TraceEventFamily::Recovery,
        audit::DurableAuditTraceFamily::ManifestCatalog => TraceEventFamily::ManifestCatalog,
        audit::DurableAuditTraceFamily::Protocol => TraceEventFamily::Protocol,
        audit::DurableAuditTraceFamily::SecurityAudit => TraceEventFamily::SecurityAudit,
        audit::DurableAuditTraceFamily::AdminAudit => TraceEventFamily::AdminAudit,
        audit::DurableAuditTraceFamily::Resource => TraceEventFamily::Resource,
        audit::DurableAuditTraceFamily::Io => TraceEventFamily::Io,
        audit::DurableAuditTraceFamily::Gpu => TraceEventFamily::Gpu,
        audit::DurableAuditTraceFamily::Transaction => TraceEventFamily::Transaction,
    }
}
