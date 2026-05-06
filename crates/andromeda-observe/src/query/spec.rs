use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ProcedureId,
};

use super::TraceEventFamily;
use crate::TraceId;

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

fn trace_query_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}
