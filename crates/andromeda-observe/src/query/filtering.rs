use andromeda_types::CatalogObjectId;

use super::{TraceEventFamily, TraceQueryLsnRange, TraceQuerySpec};
use crate::{EventEnvelope, TraceEvent};

pub(super) fn matches_filter(envelope: &EventEnvelope, spec: &TraceQuerySpec) -> bool {
    let filter = &spec.filter;
    match filter.trace_id {
        Some(trace_id) if envelope.trace_id != trace_id => {
            return false;
        }
        _ => {}
    }
    match filter.family {
        Some(family) if TraceEventFamily::of(&envelope.event) != family => {
            return false;
        }
        _ => {}
    }
    match filter.lsn_range {
        Some(range) if !matches_lsn_range(envelope, range) => {
            return false;
        }
        _ => {}
    }
    match filter.catalog_version {
        Some(catalog_version) if envelope.correlation.catalog_version != Some(catalog_version) => {
            return false;
        }
        _ => {}
    }
    if let Some(procedure_id) = filter.procedure_id {
        let expected = CatalogObjectId::new(procedure_id.get());
        if envelope.correlation.catalog_object_id != Some(expected) {
            return false;
        }
    }
    match &filter.principal {
        Some(principal) if principal_of(&envelope.event) != Some(principal.as_str()) => {
            return false;
        }
        _ => {}
    }
    true
}

fn matches_lsn_range(envelope: &EventEnvelope, range: TraceQueryLsnRange) -> bool {
    if envelope
        .correlation
        .durable_lsn
        .is_some_and(|lsn| range.contains(lsn))
    {
        return true;
    }

    match &envelope.event {
        TraceEvent::Wal(trace) => range.contains(trace.durable_lsn),
        TraceEvent::WalEvent(trace) => {
            range.contains(trace.appended_lsn) || matches_optional_lsn(trace.durable_lsn, range)
        }
        TraceEvent::CommitVisible(trace) => range.contains(trace.durable_commit_lsn),
        TraceEvent::RollbackDurable(trace) => range.contains(trace.durable_rollback_lsn),
        TraceEvent::RecoveryStartup(trace) => {
            range.contains(trace.last_durable_lsn)
                || matches_optional_lsn(trace.corruption_boundary_lsn, range)
        }
        TraceEvent::Manifest(trace) => {
            range.contains(trace.base_checkpoint_lsn)
                || range.contains(trace.required_wal_start_lsn)
        }
        TraceEvent::CompletionEmitted(trace) => matches_optional_lsn(trace.durable_lsn, range),
        TraceEvent::CorruptionBoundary(trace) => range.contains(trace.boundary_lsn),
        _ => false,
    }
}

fn matches_optional_lsn(lsn: Option<u64>, range: TraceQueryLsnRange) -> bool {
    lsn.is_some_and(|lsn| range.contains(lsn))
}

fn principal_of(event: &TraceEvent) -> Option<&str> {
    match event {
        TraceEvent::SecurityAudit(trace) => Some(trace.principal.principal_id.as_str()),
        TraceEvent::AdminOperation(trace) => Some(trace.principal.principal_id.as_str()),
        TraceEvent::Audit(trace) => Some(trace.actor.as_str()),
        _ => None,
    }
}
