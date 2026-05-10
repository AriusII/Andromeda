use andromeda_audit::DurableAuditTraceQueryPermissionMatrix;
use andromeda_error::AndromedaResult;

use super::{
    TraceEventFamily, TraceQueryMetadata, TraceQueryResult, TraceQueryRow, TraceQuerySpec,
    filtering::matches_filter,
};
use crate::InMemoryEventSink;

impl InMemoryEventSink {
    /// Run a bounded administrative trace inspection over recorded envelopes.
    pub fn inspect_trace_events(&self, spec: &TraceQuerySpec) -> AndromedaResult<TraceQueryResult> {
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
                permission_matrix: DurableAuditTraceQueryPermissionMatrix::V1_ADMIN,
            },
            rows,
        })
    }
}
