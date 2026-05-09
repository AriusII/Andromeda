use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observability::{
    EventId, TraceQueryLsnRange, TraceQueryValidation, TraceQueryValidationMessages,
    trace_query_filter_is_unbounded, validate_trace_query_parts,
};
use andromeda_types::{CatalogVersion, ProcedureId, RequestId, SessionId};

use crate::helpers::contains_sensitive_marker;
use crate::{
    AdminOperation, DurableAuditEventFamily, DurableAuditReplayBehavior, DurableAuditReplayRecord,
    DurableAuditRetentionBoundary, Permission, SurfaceScope, TraceId,
};

pub const DURABLE_AUDIT_QUERY_MAX_LIMIT: usize = 1_000;
pub const DURABLE_AUDIT_QUERY_DEFAULT_LIMIT: usize = 100;

const DURABLE_AUDIT_QUERY_VALIDATION_MESSAGES: TraceQueryValidationMessages =
    TraceQueryValidationMessages {
        limit_zero: "durable audit trace query limit must be non-zero",
        limit_exceeds_max: "durable audit trace query limit exceeds DURABLE_AUDIT_QUERY_MAX_LIMIT",
        trace_id_zero: "durable audit trace query trace_id filter must be non-zero when present",
        lsn_range_invalid: "durable audit trace query LSN range must be non-zero and start_lsn <= end_lsn",
        catalog_version_zero: "durable audit trace query catalog_version filter must be non-zero when present",
        procedure_id_zero: "durable audit trace query procedure_id filter must be non-zero when present",
        principal_empty: "durable audit trace query principal filter must be non-empty when present",
    };

/// Event-family projection supported by durable audit replay inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditTraceFamily {
    Decision,
    ProcedureInvocation,
    Wal,
    Recovery,
    ManifestCatalog,
    Protocol,
    SecurityAudit,
    AdminAudit,
    Resource,
    Io,
    Gpu,
    Transaction,
}

/// Inclusive LSN interval for durable audit replay inspection.
pub type DurableAuditTraceQueryLsnRange = TraceQueryLsnRange;

/// Durable audit replay inspection filters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DurableAuditTraceQueryFilter {
    pub trace_id: Option<TraceId>,
    pub family: Option<DurableAuditTraceFamily>,
    pub lsn_range: Option<DurableAuditTraceQueryLsnRange>,
    pub catalog_version: Option<CatalogVersion>,
    pub procedure_id: Option<ProcedureId>,
    pub principal: Option<String>,
}

impl DurableAuditTraceQueryFilter {
    pub fn is_unbounded(&self) -> bool {
        trace_query_filter_is_unbounded(
            self.trace_id.is_some(),
            self.family.is_some(),
            self.lsn_range.is_some(),
            self.catalog_version.is_some(),
            self.procedure_id.is_some(),
            self.principal.as_deref(),
        )
    }
}

/// Bounded durable audit replay inspection request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditTraceQuerySpec {
    pub filter: DurableAuditTraceQueryFilter,
    pub limit: usize,
    pub offset: usize,
    pub include_total_count: bool,
}

impl DurableAuditTraceQuerySpec {
    pub fn new(filter: DurableAuditTraceQueryFilter) -> Self {
        Self {
            filter,
            limit: DURABLE_AUDIT_QUERY_DEFAULT_LIMIT,
            offset: 0,
            include_total_count: false,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        validate_trace_query_parts(TraceQueryValidation {
            limit: self.limit,
            max_limit: DURABLE_AUDIT_QUERY_MAX_LIMIT,
            trace_id: self.filter.trace_id,
            lsn_range_valid: self
                .filter
                .lsn_range
                .map(DurableAuditTraceQueryLsnRange::is_valid),
            catalog_version: self.filter.catalog_version,
            procedure_id: self.filter.procedure_id,
            principal: self.filter.principal.as_deref(),
            messages: DURABLE_AUDIT_QUERY_VALIDATION_MESSAGES,
        })
    }
}

/// Permission matrix for durable audit replay inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditTraceQueryPermissionMatrix {
    pub surface: SurfaceScope,
    pub required_permission: Permission,
    pub audit_operation: AdminOperation,
    pub audit_required: bool,
}

impl DurableAuditTraceQueryPermissionMatrix {
    pub const V1_ADMIN: Self = Self {
        surface: SurfaceScope::Administration,
        required_permission: Permission::InspectPlans,
        audit_operation: AdminOperation::InspectPlans,
        audit_required: true,
    };

    pub const fn permits(self, surface: SurfaceScope, permission: Permission) -> bool {
        surface.same_surface(self.surface)
            && matches!(
                permission,
                Permission::InspectPlans | Permission::ManageSecurity
            )
    }
}

/// Metadata returned with durable audit replay inspection results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditTraceQueryMetadata {
    pub limit: usize,
    pub offset: usize,
    pub returned_rows: usize,
    pub total_matching_rows: Option<usize>,
    pub truncated: bool,
    pub ordered_by_event_id_ascending: bool,
    pub permission_matrix: DurableAuditTraceQueryPermissionMatrix,
}

/// Administrative inspection adapter over replayed durable-audit WAL records.
#[derive(Debug, Clone, Copy)]
pub struct DurableAuditTraceQuerySource<'a> {
    records: &'a [DurableAuditReplayRecord],
}

impl<'a> DurableAuditTraceQuerySource<'a> {
    pub const fn new(records: &'a [DurableAuditReplayRecord]) -> Self {
        Self { records }
    }

    pub fn inspect(
        &self,
        spec: &DurableAuditTraceQuerySpec,
    ) -> AndromedaResult<DurableAuditTraceQueryResult> {
        spec.validate()?;
        validate_supported_filters(spec)?;

        let mut skipped = 0usize;
        let mut total_matching = 0usize;
        let mut rows = Vec::new();
        let mut previous_matching_event_id = None;
        let mut ordered_by_event_id_ascending = true;

        for record in self.records {
            record.validate()?;
            let row = DurableAuditTraceQueryRow::from_replay_record(record)?;
            if !row.matches(spec) {
                continue;
            }

            match previous_matching_event_id {
                Some(previous_event_id) if row.event_id < previous_event_id => {
                    ordered_by_event_id_ascending = false;
                },
                _ => {},
            }
            previous_matching_event_id = Some(row.event_id);

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
            metadata: DurableAuditTraceQueryMetadata {
                limit: spec.limit,
                offset: spec.offset,
                returned_rows,
                total_matching_rows: spec.include_total_count.then_some(total_matching),
                truncated: total_matching.saturating_sub(spec.offset) > returned_rows,
                ordered_by_event_id_ascending,
                permission_matrix: DurableAuditTraceQueryPermissionMatrix::V1_ADMIN,
            },
            rows,
        })
    }

    pub fn query(
        &self,
        spec: &DurableAuditTraceQuerySpec,
    ) -> AndromedaResult<DurableAuditTraceQueryResult> {
        self.inspect(spec)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditTraceQueryResult {
    pub metadata: DurableAuditTraceQueryMetadata,
    pub rows: Vec<DurableAuditTraceQueryRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditTraceQueryRow {
    pub event_id: EventId,
    pub trace_id: TraceId,
    pub family: DurableAuditTraceFamily,
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
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
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

    fn matches(&self, spec: &DurableAuditTraceQuerySpec) -> bool {
        let filter = &spec.filter;
        match filter.trace_id {
            Some(trace_id) if self.trace_id != trace_id => {
                return false;
            },
            _ => {},
        }
        match filter.family {
            Some(family) if self.family != family => {
                return false;
            },
            _ => {},
        }
        match filter.lsn_range {
            Some(range) if !matches_lsn_range(self, range) => {
                return false;
            },
            _ => {},
        }
        match &filter.principal {
            Some(principal) if self.principal_id != *principal => {
                return false;
            },
            _ => {},
        }
        true
    }
}

fn validate_supported_filters(spec: &DurableAuditTraceQuerySpec) -> AndromedaResult<()> {
    match &spec.filter.principal {
        Some(principal) if contains_sensitive_marker(principal) => {
            return Err(durable_query_error(
                "durable audit trace inspection principal filter must not contain secret evidence",
            ));
        },
        _ => {},
    }
    if spec.filter.catalog_version.is_some() {
        return Err(durable_query_error(
            "durable audit trace inspection cannot filter catalog_version because the durable audit journal does not carry catalog correlation",
        ));
    }
    if spec.filter.procedure_id.is_some() {
        return Err(durable_query_error(
            "durable audit trace inspection cannot filter procedure_id because the durable audit journal does not carry catalog object correlation",
        ));
    }
    Ok(())
}

fn matches_lsn_range(
    row: &DurableAuditTraceQueryRow,
    range: DurableAuditTraceQueryLsnRange,
) -> bool {
    range.contains(row.record_lsn) || range.contains(row.durable_lsn)
}

fn trace_family_for_replay_record(
    record: &DurableAuditReplayRecord,
) -> AndromedaResult<DurableAuditTraceFamily> {
    Ok(match record.report.identity.family {
        DurableAuditEventFamily::SecurityDecision => DurableAuditTraceFamily::SecurityAudit,
        DurableAuditEventFamily::AdminDecision
        | DurableAuditEventFamily::HadrDecision
        | DurableAuditEventFamily::BackupDecision
        | DurableAuditEventFamily::RestoreDecision
        | DurableAuditEventFamily::ForensicDecision
        | DurableAuditEventFamily::GenericAudit => DurableAuditTraceFamily::AdminAudit,
        DurableAuditEventFamily::AdmissionDecision => DurableAuditTraceFamily::Protocol,
        DurableAuditEventFamily::CatalogDecision => DurableAuditTraceFamily::ManifestCatalog,
        DurableAuditEventFamily::RecoveryDecision => recovery_trace_family(&record.event_kind)?,
    })
}

fn recovery_trace_family(event_kind: &str) -> AndromedaResult<DurableAuditTraceFamily> {
    match event_kind {
        "RecoveryStartup" => Ok(DurableAuditTraceFamily::Recovery),
        "WalAppend" | "WalFlush" | "CommitVisible" | "RollbackDurable" | "CorruptionBoundary" => {
            Ok(DurableAuditTraceFamily::Wal)
        },
        _ => Err(durable_query_error(
            "durable audit recovery replay record carries unknown event_kind evidence",
        )),
    }
}

fn redact_sensitive_evidence(text: &str) -> String {
    if contains_sensitive_marker(text) {
        "[redacted-sensitive-evidence]".to_string()
    } else {
        text.to_string()
    }
}

fn durable_query_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}
