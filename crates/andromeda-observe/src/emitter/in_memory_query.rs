use andromeda_core::{
    CatalogObjectId, CatalogVersion, ContractHash, RequestId, SessionId, TransactionId,
};

use crate::{
    TraceId,
    events::{EventEnvelope, EventId, InMemoryEventSink, TraceEvent},
};

impl InMemoryEventSink {
    fn events_matching(
        &self,
        mut predicate: impl FnMut(&EventEnvelope) -> bool,
    ) -> Vec<&EventEnvelope> {
        self.events()
            .iter()
            .filter(|envelope| predicate(envelope))
            .collect()
    }

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
        self.events_matching(|envelope| envelope.trace_id == trace_id)
    }

    /// All recorded envelopes whose correlation carries the given request id.
    pub fn events_for_request(&self, request_id: RequestId) -> Vec<&EventEnvelope> {
        self.events_matching(|envelope| envelope.correlation.request_id == Some(request_id))
    }

    /// All recorded envelopes whose correlation carries the given session id.
    pub fn events_for_session(&self, session_id: SessionId) -> Vec<&EventEnvelope> {
        self.events_matching(|envelope| envelope.correlation.session_id == Some(session_id))
    }

    /// All recorded envelopes whose correlation carries the given transaction
    /// id.
    pub fn events_for_transaction(&self, transaction_id: TransactionId) -> Vec<&EventEnvelope> {
        self.events_matching(|envelope| envelope.correlation.transaction_id == Some(transaction_id))
    }

    /// All recorded envelopes whose correlation carries the given contract
    /// hash.
    pub fn events_for_contract(&self, contract_hash: ContractHash) -> Vec<&EventEnvelope> {
        self.events_matching(|envelope| envelope.correlation.contract_hash == Some(contract_hash))
    }

    /// All recorded envelopes whose correlation carries the given catalog
    /// version.
    pub fn events_for_catalog_version(
        &self,
        catalog_version: CatalogVersion,
    ) -> Vec<&EventEnvelope> {
        self.events_matching(|envelope| {
            envelope.correlation.catalog_version == Some(catalog_version)
        })
    }

    /// All recorded envelopes whose correlation carries the given catalog
    /// object id.
    pub fn events_for_catalog_object(
        &self,
        catalog_object_id: CatalogObjectId,
    ) -> Vec<&EventEnvelope> {
        self.events_matching(|envelope| {
            envelope.correlation.catalog_object_id == Some(catalog_object_id)
        })
    }

    /// Locate a single recorded envelope by `EventId`.
    pub fn find_event(&self, event_id: EventId) -> Option<&EventEnvelope> {
        self.events()
            .iter()
            .find(|envelope| envelope.event_id == event_id)
    }

    /// All recorded envelopes whose payload is a `TransactionTransition`
    /// trace. Useful for forensic timelines that need every observed phase
    /// crossing for a transaction without filtering on `kind()` separately.
    pub fn transaction_transition_events(&self) -> Vec<&EventEnvelope> {
        self.events_matching(|envelope| {
            matches!(envelope.event, TraceEvent::TransactionTransition(_))
        })
    }

    /// All recorded envelopes whose payload is an `ExecutionTransition`
    /// trace. Mirrors [`Self::transaction_transition_events`] for the
    /// invocation-side projection.
    pub fn execution_transition_events(&self) -> Vec<&EventEnvelope> {
        self.events_matching(|envelope| {
            matches!(envelope.event, TraceEvent::ExecutionTransition(_))
        })
    }
}
