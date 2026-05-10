use andromeda_audit::DurableAuditTraceQueryPermissionMatrix;

use super::TraceEventFamily;
use crate::EventEnvelope;

/// Metadata returned with every trace query result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceQueryMetadata {
    pub limit: usize,
    pub offset: usize,
    pub returned_rows: usize,
    pub total_matching_rows: Option<usize>,
    pub truncated: bool,
    pub ordered_by_event_id_ascending: bool,
    pub permission_matrix: DurableAuditTraceQueryPermissionMatrix,
}

/// Single typed result row.
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
