use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ProcedureId};

use crate::TraceId;

/// Maximum rows per trace query.
pub const TRACE_QUERY_MAX_LIMIT: usize = 1_000;

/// Default trace query row limit.
pub const TRACE_QUERY_DEFAULT_LIMIT: usize = 100;

const TRACE_QUERY_VALIDATION_MESSAGES: TraceQueryValidationMessages =
    TraceQueryValidationMessages {
        limit_zero: "trace query limit must be non-zero",
        limit_exceeds_max: "trace query limit exceeds TRACE_QUERY_MAX_LIMIT",
        trace_id_zero: "trace query trace_id filter must be non-zero when present",
        lsn_range_invalid: "trace query LSN range must be non-zero and start_lsn <= end_lsn",
        catalog_version_zero: "trace query catalog_version filter must be non-zero when present",
        procedure_id_zero: "trace query procedure_id filter must be non-zero when present",
        principal_empty: "trace query principal filter must be non-empty when present",
    };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceQueryValidationMessages {
    pub limit_zero: &'static str,
    pub limit_exceeds_max: &'static str,
    pub trace_id_zero: &'static str,
    pub lsn_range_invalid: &'static str,
    pub catalog_version_zero: &'static str,
    pub procedure_id_zero: &'static str,
    pub principal_empty: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceQueryValidation<'a> {
    pub limit: usize,
    pub max_limit: usize,
    pub trace_id: Option<TraceId>,
    pub lsn_range_valid: Option<bool>,
    pub catalog_version: Option<CatalogVersion>,
    pub procedure_id: Option<ProcedureId>,
    pub principal: Option<&'a str>,
    pub messages: TraceQueryValidationMessages,
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

pub trait TraceEventFamilyClassified {
    fn trace_event_family(&self) -> TraceEventFamily;
}

impl TraceEventFamily {
    pub fn of(event: &(impl TraceEventFamilyClassified + ?Sized)) -> Self {
        event.trace_event_family()
    }
}

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
        validate_trace_query_parts(TraceQueryValidation {
            limit: self.limit,
            max_limit: TRACE_QUERY_MAX_LIMIT,
            trace_id: self.filter.trace_id,
            lsn_range_valid: self.filter.lsn_range.map(TraceQueryLsnRange::is_valid),
            catalog_version: self.filter.catalog_version,
            procedure_id: self.filter.procedure_id,
            principal: self.filter.principal.as_deref(),
            messages: TRACE_QUERY_VALIDATION_MESSAGES,
        })
    }
}

pub fn trace_query_filter_is_unbounded(
    has_trace_id: bool,
    has_family: bool,
    has_lsn_range: bool,
    has_catalog_version: bool,
    has_procedure_id: bool,
    principal: Option<&str>,
) -> bool {
    !has_trace_id
        && !has_family
        && !has_lsn_range
        && !has_catalog_version
        && !has_procedure_id
        && principal.is_none_or(|principal| principal.trim().is_empty())
}

pub fn validate_trace_query_parts(input: TraceQueryValidation<'_>) -> AndromedaResult<()> {
    if input.limit == 0 {
        return Err(trace_query_error(input.messages.limit_zero));
    }
    if input.limit > input.max_limit {
        return Err(trace_query_error(input.messages.limit_exceeds_max));
    }
    if let Some(trace_id) = input.trace_id
        && trace_id.is_zero()
    {
        return Err(trace_query_error(input.messages.trace_id_zero));
    }

    if let Some(false) = input.lsn_range_valid {
        return Err(trace_query_error(input.messages.lsn_range_invalid));
    }

    if let Some(catalog_version) = input.catalog_version
        && catalog_version.get() == 0
    {
        return Err(trace_query_error(input.messages.catalog_version_zero));
    }

    if let Some(procedure_id) = input.procedure_id
        && procedure_id.get() == 0
    {
        return Err(trace_query_error(input.messages.procedure_id_zero));
    }

    if let Some(principal) = input.principal
        && principal.trim().is_empty()
    {
        return Err(trace_query_error(input.messages.principal_empty));
    }
    Ok(())
}

fn trace_query_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
