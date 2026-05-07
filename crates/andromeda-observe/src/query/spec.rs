use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ProcedureId};

use super::TraceEventFamily;
use crate::TraceId;

/// Maximum rows per trace query.
pub const TRACE_QUERY_MAX_LIMIT: usize = 1_000;

/// Default trace query row limit.
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

/// Typed trace query filters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceQueryFilter {
    pub trace_id: Option<TraceId>,
    pub family: Option<TraceEventFamily>,
    pub lsn_range: Option<TraceQueryLsnRange>,
    pub catalog_version: Option<CatalogVersion>,
    /// Matched through `EventCorrelation.catalog_object_id`.
    pub procedure_id: Option<ProcedureId>,
    /// Matched against audit payload principal evidence.
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
                .is_none_or(|principal| principal.trim().is_empty())
    }
}

/// Bounded trace query request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceQuerySpec {
    pub filter: TraceQueryFilter,
    /// Maximum rows to return after filtering.
    pub limit: usize,
    /// Matching rows to skip before collection.
    pub offset: usize,
    /// Whether to compute the full matching count.
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
