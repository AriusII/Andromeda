use andromeda_core::CatalogObjectId;

use super::{TraceEventFamily, TraceQuerySpec};
use crate::{EventEnvelope, TraceEvent};

pub(super) fn matches_filter(envelope: &EventEnvelope, spec: &TraceQuerySpec) -> bool {
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
