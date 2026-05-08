use andromeda_types::CatalogObjectId;

use super::{TraceEventFamily, TraceQueryLsnRange, TraceQuerySpec};
use crate::EventEnvelope;

mod lsn;
mod principal;

use lsn::matches_lsn_range;
use principal::principal_of;

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
