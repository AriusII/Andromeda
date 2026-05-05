//! Typed administration-only trace query contract.
//!
//! This module intentionally models an observability read surface, not an
//! application data access path. It accepts a closed set of typed filters over
//! recorded [`EventEnvelope`] metadata and payload evidence. It does not expose
//! SQL, does not define an application `Procedure` surface, and does not make
//! JSON a runtime wire format. Operators may render returned rows as diagnostic
//! CLI JSON outside this contract.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ProcedureId,
};

use crate::{
    AdminOperation, EventEnvelope, InMemoryEventSink, Permission, SurfaceScope, TraceEvent, TraceId,
};

/// Maximum rows a V1 operator trace query may return.
pub const TRACE_QUERY_MAX_LIMIT: usize = 1_000;

/// Default bounded row limit when a caller does not request one explicitly.
pub const TRACE_QUERY_DEFAULT_LIMIT: usize = 100;

/// Inclusive LSN interval for trace event filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceQueryLsnRange {
    pub start_lsn: u64,
    pub end_lsn: u64,
}

impl TraceQueryLsnRange {
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

/// Closed event-family taxonomy used by the administration trace query surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TraceEventFamily {
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

impl TraceEventFamily {
    pub fn of(event: &TraceEvent) -> Self {
        match event {
            TraceEvent::Decision(_) => Self::Decision,
            TraceEvent::Invocation(_) | TraceEvent::ExecutionTransition(_) => {
                Self::ProcedureInvocation
            }
            TraceEvent::Wal(_)
            | TraceEvent::WalEvent(_)
            | TraceEvent::CommitVisible(_)
            | TraceEvent::RollbackDurable(_)
            | TraceEvent::CorruptionBoundary(_) => Self::Wal,
            TraceEvent::RecoveryStartup(_) => Self::Recovery,
            TraceEvent::Manifest(_) | TraceEvent::CatalogMutation(_) => Self::ManifestCatalog,
            TraceEvent::FrameRejection(_)
            | TraceEvent::StreamRoleRejection(_)
            | TraceEvent::Backpressure(_)
            | TraceEvent::CompletionEmitted(_)
            | TraceEvent::ContractRejected(_)
            | TraceEvent::UnsupportedVersion(_)
            | TraceEvent::SchemaLayoutDecision(_) => Self::Protocol,
            TraceEvent::AuthorizationDenied(_) | TraceEvent::SecurityAudit(_) => {
                Self::SecurityAudit
            }
            TraceEvent::AdminOperation(_) | TraceEvent::Audit(_) => Self::AdminAudit,
            TraceEvent::Resource(_) => Self::Resource,
            TraceEvent::IoPlacementDecision(_) | TraceEvent::IoBudgetDecision(_) => Self::Io,
            TraceEvent::GpuPolicyDecision(_) => Self::Gpu,
            TraceEvent::Mvcc(_) | TraceEvent::TransactionTransition(_) => Self::Transaction,
        }
    }
}

/// Allowed typed filters for V1 operator trace queries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceQueryFilter {
    pub trace_id: Option<TraceId>,
    pub family: Option<TraceEventFamily>,
    pub lsn_range: Option<TraceQueryLsnRange>,
    pub catalog_version: Option<CatalogVersion>,
    /// Procedure identity is matched through `EventCorrelation.catalog_object_id`
    /// because V1 trace envelopes do not yet carry a dedicated procedure id
    /// field. Producers that want this filter to match must set the procedure's
    /// catalog object id in the envelope correlation.
    pub procedure_id: Option<ProcedureId>,
    /// Principal identity is matched only against audit payloads that explicitly
    /// carry a `UserPrincipal` or legacy `AuditTrace.actor` evidence.
    pub principal: Option<String>,
}

impl TraceQueryFilter {
    pub fn is_unbounded(&self) -> bool {
        self.trace_id.is_none()
            && self.family.is_none()
            && self.lsn_range.is_none()
            && self.catalog_version.is_none()
            && self.procedure_id.is_none()
            && self
                .principal
                .as_ref()
                .map_or(true, |principal| principal.trim().is_empty())
    }
}

/// Fully bounded V1 trace query request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceQuerySpec {
    pub filter: TraceQueryFilter,
    /// Maximum rows to return after filtering. Must be 1..=TRACE_QUERY_MAX_LIMIT.
    pub limit: usize,
    /// Number of matching rows to skip before collection. This is deterministic
    /// over ascending `EventId` order and is bounded by the in-memory source size.
    pub offset: usize,
    /// Whether the query should compute the full matching count. If false,
    /// `total_matching_rows` is `None` and only returned row count is reported.
    pub include_total_count: bool,
}

impl TraceQuerySpec {
    pub fn new(filter: TraceQueryFilter) -> Self {
        Self {
            filter,
            limit: TRACE_QUERY_DEFAULT_LIMIT,
            offset: 0,
            include_total_count: false,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.limit == 0 {
            return Err(trace_query_error("trace query limit must be non-zero"));
        }
        if self.limit > TRACE_QUERY_MAX_LIMIT {
            return Err(trace_query_error(
                "trace query limit exceeds TRACE_QUERY_MAX_LIMIT",
            ));
        }
        if let Some(trace_id) = self.filter.trace_id
            && trace_id.is_zero()
        {
            return Err(trace_query_error(
                "trace query trace_id filter must be non-zero when present",
            ));
        }
        if let Some(range) = self.filter.lsn_range
            && !range.is_valid()
        {
            return Err(trace_query_error(
                "trace query LSN range must be non-zero and start_lsn <= end_lsn",
            ));
        }
        if let Some(catalog_version) = self.filter.catalog_version
            && catalog_version.get() == 0
        {
            return Err(trace_query_error(
                "trace query catalog_version filter must be non-zero when present",
            ));
        }
        if let Some(procedure_id) = self.filter.procedure_id
            && procedure_id.get() == 0
        {
            return Err(trace_query_error(
                "trace query procedure_id filter must be non-zero when present",
            ));
        }
        if let Some(principal) = &self.filter.principal
            && principal.trim().is_empty()
        {
            return Err(trace_query_error(
                "trace query principal filter must be non-empty when present",
            ));
        }
        Ok(())
    }
}

/// Permission/audit matrix for V1 administration trace access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceQueryPermissionMatrix {
    pub surface: SurfaceScope,
    pub required_permission: Permission,
    pub audit_operation: AdminOperation,
    pub audit_required: bool,
}

impl TraceQueryPermissionMatrix {
    /// V1 trace query is administration-only and uses the existing diagnostics
    /// permission until IAM adds a dedicated trace-read permission.
    pub const V1_ADMIN: Self = Self {
        surface: SurfaceScope::Administration,
        required_permission: Permission::InspectPlans,
        audit_operation: AdminOperation::InspectPlans,
        audit_required: true,
    };

    pub const fn permits(self, surface: SurfaceScope, permission: Permission) -> bool {
        surface as u8 == self.surface as u8
            && matches!(
                permission,
                Permission::InspectPlans | Permission::ManageSecurity
            )
    }
}

/// Metadata returned with every trace query result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceQueryMetadata {
    pub limit: usize,
    pub offset: usize,
    pub returned_rows: usize,
    pub total_matching_rows: Option<usize>,
    pub truncated: bool,
    pub ordered_by_event_id_ascending: bool,
    pub permission_matrix: TraceQueryPermissionMatrix,
}

/// Single result row. The row preserves the typed envelope; callers must choose
/// any diagnostic formatting explicitly at the CLI/export layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceQueryRow {
    pub envelope: EventEnvelope,
    pub family: TraceEventFamily,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceQueryResult {
    pub metadata: TraceQueryMetadata,
    pub rows: Vec<TraceQueryRow>,
}

impl InMemoryEventSink {
    /// Run a deterministic, bounded administration trace query over recorded
    /// in-memory envelopes.
    pub fn query_trace_events(&self, spec: &TraceQuerySpec) -> AndromedaResult<TraceQueryResult> {
        spec.validate()?;

        let mut skipped = 0usize;
        let mut total_matching = 0usize;
        let mut rows = Vec::new();

        for envelope in self
            .events()
            .iter()
            .filter(|event| matches_filter(event, spec))
        {
            total_matching = total_matching.saturating_add(1);
            if skipped < spec.offset {
                skipped = skipped.saturating_add(1);
                continue;
            }
            if rows.len() < spec.limit {
                rows.push(TraceQueryRow {
                    envelope: envelope.clone(),
                    family: TraceEventFamily::of(&envelope.event),
                });
            }
        }

        let returned_rows = rows.len();
        Ok(TraceQueryResult {
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
}

fn matches_filter(envelope: &EventEnvelope, spec: &TraceQuerySpec) -> bool {
    let filter = &spec.filter;
    if let Some(trace_id) = filter.trace_id
        && envelope.trace_id != trace_id
    {
        return false;
    }
    if let Some(family) = filter.family
        && TraceEventFamily::of(&envelope.event) != family
    {
        return false;
    }
    if let Some(range) = filter.lsn_range
        && !event_lsns(envelope)
            .into_iter()
            .any(|lsn| range.contains(lsn))
    {
        return false;
    }
    if let Some(catalog_version) = filter.catalog_version
        && envelope.correlation.catalog_version != Some(catalog_version)
    {
        return false;
    }
    if let Some(procedure_id) = filter.procedure_id {
        let expected = CatalogObjectId::new(procedure_id.get());
        if envelope.correlation.catalog_object_id != Some(expected) {
            return false;
        }
    }
    if let Some(principal) = &filter.principal
        && principal_of(&envelope.event) != Some(principal.as_str())
    {
        return false;
    }
    true
}

fn event_lsns(envelope: &EventEnvelope) -> Vec<u64> {
    let mut lsns = Vec::new();
    if let Some(lsn) = envelope.correlation.durable_lsn {
        lsns.push(lsn);
    }
    match &envelope.event {
        TraceEvent::Wal(trace) => lsns.push(trace.durable_lsn),
        TraceEvent::WalEvent(trace) => {
            lsns.push(trace.appended_lsn);
            if let Some(lsn) = trace.durable_lsn {
                lsns.push(lsn);
            }
        }
        TraceEvent::CommitVisible(trace) => lsns.push(trace.durable_commit_lsn),
        TraceEvent::RollbackDurable(trace) => lsns.push(trace.durable_rollback_lsn),
        TraceEvent::RecoveryStartup(trace) => {
            lsns.push(trace.last_durable_lsn);
            if let Some(lsn) = trace.corruption_boundary_lsn {
                lsns.push(lsn);
            }
        }
        TraceEvent::Manifest(trace) => {
            lsns.push(trace.base_checkpoint_lsn);
            lsns.push(trace.required_wal_start_lsn);
        }
        TraceEvent::CompletionEmitted(trace) => {
            if let Some(lsn) = trace.durable_lsn {
                lsns.push(lsn);
            }
        }
        TraceEvent::CorruptionBoundary(trace) => lsns.push(trace.boundary_lsn),
        _ => {}
    }
    lsns
}

fn principal_of(event: &TraceEvent) -> Option<&str> {
    match event {
        TraceEvent::SecurityAudit(trace) => Some(trace.principal.principal_id.as_str()),
        TraceEvent::AdminOperation(trace) => Some(trace.principal.principal_id.as_str()),
        TraceEvent::Audit(trace) => Some(trace.actor.as_str()),
        _ => None,
    }
}

fn trace_query_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

#[cfg(test)]
mod tests {
    use andromeda_core::{CatalogObjectId, CatalogVersion, ProcedureId, RequestId, SessionId};

    use super::*;
    use crate::{
        BackpressureTrace, CertificateIdentity, EventCorrelation, EventEmitter, InMemoryEventSink,
        ProtocolCorrelation, ProtocolEventScope, SecurityAuditOutcome, SecurityAuditTrace,
        UserPrincipal, UserPrincipalKind, WalEventTrace, WalOperation,
    };

    fn protocol_event(trace_id: u128) -> TraceEvent {
        TraceEvent::Backpressure(BackpressureTrace {
            trace_id: TraceId::new(trace_id),
            scope: ProtocolEventScope::Connection,
            protocol: ProtocolCorrelation::empty(),
            retry_after_micros: Some(10),
            pending_units: None,
            limit_units: None,
            reason: "bounded queue pressure".to_string(),
        })
    }

    fn wal_event(trace_id: u128, lsn: u64) -> TraceEvent {
        TraceEvent::WalEvent(WalEventTrace {
            trace_id: TraceId::new(trace_id),
            transaction_id: None,
            operation: WalOperation::Append,
            appended_lsn: lsn,
            durable_lsn: None,
        })
    }

    fn security_event(trace_id: u128, principal: &str) -> TraceEvent {
        TraceEvent::SecurityAudit(
            SecurityAuditTrace::new(
                TraceId::new(trace_id),
                SurfaceScope::Administration,
                CertificateIdentity::new(
                    "fingerprint-for-trace-query",
                    "CN=trace-query-admin",
                    SurfaceScope::Administration,
                )
                .unwrap(),
                UserPrincipal::new(principal, UserPrincipalKind::Human).unwrap(),
                Permission::InspectPlans,
                SecurityAuditOutcome::Allowed,
                "trace query access authorized",
            )
            .unwrap(),
        )
    }

    #[test]
    fn trace_query_spec_rejects_unbounded_limits_and_zero_filters() {
        let mut spec = TraceQuerySpec::new(TraceQueryFilter::default());
        spec.limit = 0;
        assert!(spec.validate().is_err());

        spec.limit = TRACE_QUERY_MAX_LIMIT + 1;
        assert!(spec.validate().is_err());

        spec.limit = TRACE_QUERY_DEFAULT_LIMIT;
        spec.filter.trace_id = Some(TraceId::new(0));
        assert!(spec.validate().is_err());

        spec.filter.trace_id = None;
        spec.filter.lsn_range = Some(TraceQueryLsnRange::new(20, 10));
        assert!(spec.validate().is_err());
    }

    #[test]
    fn trace_query_filters_by_family_lsn_catalog_procedure_and_principal() {
        let mut emitter = EventEmitter::new(InMemoryEventSink::new());

        emitter
            .emit(EventCorrelation::empty(), protocol_event(101))
            .unwrap();

        let mut wal_corr = EventCorrelation::empty();
        wal_corr.catalog_version = Some(CatalogVersion::new(7));
        wal_corr.catalog_object_id = Some(CatalogObjectId::new(42));
        emitter.emit(wal_corr, wal_event(102, 500)).unwrap();

        let mut audit_corr = EventCorrelation::empty();
        audit_corr.request_id = Some(RequestId::new(1));
        audit_corr.session_id = Some(SessionId::new(2));
        emitter
            .emit(audit_corr, security_event(103, "principal:operator"))
            .unwrap();

        let sink = emitter.sink();

        let mut spec = TraceQuerySpec::new(TraceQueryFilter {
            family: Some(TraceEventFamily::Wal),
            lsn_range: Some(TraceQueryLsnRange::new(400, 600)),
            catalog_version: Some(CatalogVersion::new(7)),
            procedure_id: Some(ProcedureId::new(42)),
            ..TraceQueryFilter::default()
        });
        spec.include_total_count = true;

        let result = sink.query_trace_events(&spec).unwrap();
        assert_eq!(result.metadata.returned_rows, 1);
        assert_eq!(result.metadata.total_matching_rows, Some(1));
        assert_eq!(result.rows[0].family, TraceEventFamily::Wal);

        let principal_spec = TraceQuerySpec::new(TraceQueryFilter {
            principal: Some("principal:operator".to_string()),
            ..TraceQueryFilter::default()
        });
        let result = sink.query_trace_events(&principal_spec).unwrap();
        assert_eq!(result.metadata.returned_rows, 1);
        assert_eq!(result.rows[0].family, TraceEventFamily::SecurityAudit);
    }

    #[test]
    fn trace_query_limit_offset_and_permission_matrix_are_bounded_and_deterministic() {
        let mut emitter = EventEmitter::new(InMemoryEventSink::new());
        emitter
            .emit(EventCorrelation::empty(), protocol_event(101))
            .unwrap();
        emitter
            .emit(EventCorrelation::empty(), protocol_event(102))
            .unwrap();
        emitter
            .emit(EventCorrelation::empty(), protocol_event(103))
            .unwrap();

        let mut spec = TraceQuerySpec::new(TraceQueryFilter {
            family: Some(TraceEventFamily::Protocol),
            ..TraceQueryFilter::default()
        });
        spec.limit = 1;
        spec.offset = 1;
        spec.include_total_count = true;

        let result = emitter.sink().query_trace_events(&spec).unwrap();
        assert_eq!(result.metadata.returned_rows, 1);
        assert_eq!(result.metadata.total_matching_rows, Some(3));
        assert!(result.metadata.truncated);
        assert!(result.metadata.ordered_by_event_id_ascending);
        assert_eq!(result.rows[0].envelope.event_id.get(), 2);

        let matrix = result.metadata.permission_matrix;
        assert!(matrix.audit_required);
        assert!(matrix.permits(SurfaceScope::Administration, Permission::InspectPlans));
        assert!(matrix.permits(SurfaceScope::Administration, Permission::ManageSecurity));
        assert!(!matrix.permits(SurfaceScope::Application, Permission::InspectPlans));
        assert!(!matrix.permits(SurfaceScope::MonitoringAgent, Permission::InspectPlans));
    }
}
