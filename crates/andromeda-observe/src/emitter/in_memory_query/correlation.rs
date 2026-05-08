use andromeda_types::{
    CatalogObjectId, CatalogVersion, ContractHash, RequestId, SessionId, TransactionId,
};

use super::events_matching;
use crate::{
    TraceId,
    events::{EventEnvelope, EventId, InMemoryEventSink},
};

impl InMemoryEventSink {
    /// Number of recorded envelopes.
    pub fn len(&self) -> usize {
        self.events().len()
    }

    /// Whether the sink has any recorded envelopes.
    pub fn is_empty(&self) -> bool {
        self.events().is_empty()
    }

    /// All recorded envelopes whose `trace_id` matches `trace_id`.
    pub fn events_for_trace(&self, trace_id: TraceId) -> Vec<&EventEnvelope> {
        events_matching(self, |envelope| envelope.trace_id == trace_id)
    }

    /// All recorded envelopes whose correlation carries the given request id.
    pub fn events_for_request(&self, request_id: RequestId) -> Vec<&EventEnvelope> {
        events_matching(self, |envelope| {
            envelope.correlation.request_id == Some(request_id)
        })
    }

    /// All recorded envelopes whose correlation carries the given session id.
    pub fn events_for_session(&self, session_id: SessionId) -> Vec<&EventEnvelope> {
        events_matching(self, |envelope| {
            envelope.correlation.session_id == Some(session_id)
        })
    }

    /// All recorded envelopes whose correlation carries the given transaction
    /// id.
    pub fn events_for_transaction(&self, transaction_id: TransactionId) -> Vec<&EventEnvelope> {
        events_matching(self, |envelope| {
            envelope.correlation.transaction_id == Some(transaction_id)
        })
    }

    /// All recorded envelopes whose correlation carries the given contract
    /// hash.
    pub fn events_for_contract(&self, contract_hash: ContractHash) -> Vec<&EventEnvelope> {
        events_matching(self, |envelope| {
            envelope.correlation.contract_hash == Some(contract_hash)
        })
    }

    /// All recorded envelopes whose correlation carries the given catalog
    /// version.
    pub fn events_for_catalog_version(
        &self,
        catalog_version: CatalogVersion,
    ) -> Vec<&EventEnvelope> {
        events_matching(self, |envelope| {
            envelope.correlation.catalog_version == Some(catalog_version)
        })
    }

    /// All recorded envelopes whose correlation carries the given catalog
    /// object id.
    pub fn events_for_catalog_object(
        &self,
        catalog_object_id: CatalogObjectId,
    ) -> Vec<&EventEnvelope> {
        events_matching(self, |envelope| {
            envelope.correlation.catalog_object_id == Some(catalog_object_id)
        })
    }

    /// Locate a single recorded envelope by `EventId`.
    pub fn find_event(&self, event_id: EventId) -> Option<&EventEnvelope> {
        self.events()
            .iter()
            .find(|envelope| envelope.event_id == event_id)
    }
}
