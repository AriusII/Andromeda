//! Runtime event emission abstraction.
//!
//! `EventSink` is the typed write surface (defined in [`crate::events`]). The
//! [`EventEmitter`] wraps any `EventSink` and adds:
//!
//! * monotonic, non-zero [`EventId`] allocation,
//! * pre-validation through [`EventEnvelope::new`] (no silent drops),
//! * accepted vs rejected counters so callers can prove that emission failures
//!   were observed rather than swallowed,
//! * an exhaustion guard once the `u128` event-id space is consumed.
//!
//! This is a V0 in-process scaffold. It is not a durable log, not a fan-out
//! bus, and intentionally has no async surface. Durability claims must come
//! from the storage/WAL layers, never from a sink that lives in RAM.
//!
//! In-memory query helpers on [`InMemoryEventSink`] are colocated here so
//! tests can pivot recorded envelopes by trace, request, session, transaction,
//! contract, catalog, or correlation slice without re-implementing the index.

use andromeda_core::{
    AndromedaResult, CatalogObjectId, CatalogVersion, ContractHash, RequestId, SessionId,
    TransactionId,
};

use crate::{
    events::{
        observe_error, EventCorrelation, EventEnvelope, EventId, EventSink, InMemoryEventSink,
        TraceEvent,
    },
    TraceId,
};

/// Runtime emitter that allocates monotonic `EventId`s and forwards validated
/// envelopes to an underlying [`EventSink`].
///
/// The emitter never drops events silently. Both envelope validation failures
/// and sink failures are counted in `rejected_count` and surfaced to the
/// caller through `AndromedaResult`. The allocator preserves the failed
/// `EventId` for the next attempt so the recorded sequence in the sink stays
/// contiguous on success paths.
#[derive(Debug)]
pub struct EventEmitter<S: EventSink> {
    sink: S,
    /// Next `EventId` value to attempt. `None` means the `u128` id space is
    /// exhausted and the emitter has been poisoned against further emission.
    next_event_id: Option<u128>,
    last_event_id: Option<EventId>,
    accepted_count: u64,
    rejected_count: u64,
}

impl<S: EventSink> EventEmitter<S> {
    /// Build an emitter that allocates `EventId`s starting at `1`.
    pub const fn new(sink: S) -> Self {
        Self {
            sink,
            next_event_id: Some(1),
            last_event_id: None,
            accepted_count: 0,
            rejected_count: 0,
        }
    }

    /// Build an emitter whose first allocated `EventId` is `start`.
    ///
    /// `start` must be non-zero; `EventEnvelope::validate` requires a non-zero
    /// `EventId` and the emitter will never silently coerce a zero seed.
    pub fn with_starting_event_id(sink: S, start: u128) -> AndromedaResult<Self> {
        if start == 0 {
            return Err(observe_error(
                "EventEmitter starting event_id must be non-zero",
            ));
        }
        Ok(Self {
            sink,
            next_event_id: Some(start),
            last_event_id: None,
            accepted_count: 0,
            rejected_count: 0,
        })
    }

    /// Borrow the underlying sink (read-only). The sink retains responsibility
    /// for its own queries; the emitter does not duplicate sink state.
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// Borrow the underlying sink mutably. Direct sink mutation bypasses the
    /// emitter's id allocator and counters; prefer [`Self::emit`].
    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    /// Consume the emitter and return ownership of the underlying sink.
    pub fn into_sink(self) -> S {
        self.sink
    }

    /// `EventId` that will be allocated by the next [`Self::emit`] attempt, if
    /// the id space is not exhausted.
    pub fn peek_next_event_id(&self) -> Option<EventId> {
        self.next_event_id.map(EventId::new)
    }

    /// `EventId` of the last successfully accepted event, if any.
    pub fn last_event_id(&self) -> Option<EventId> {
        self.last_event_id
    }

    /// Count of envelopes accepted by the sink.
    pub fn accepted_count(&self) -> u64 {
        self.accepted_count
    }

    /// Count of emit attempts rejected (validation failure or sink failure).
    /// Combined with `accepted_count` this proves no attempt was silently
    /// swallowed.
    pub fn rejected_count(&self) -> u64 {
        self.rejected_count
    }

    /// True once the `u128` event-id space has been consumed; further `emit`
    /// calls return an error rather than wrapping or duplicating ids.
    pub fn is_exhausted(&self) -> bool {
        self.next_event_id.is_none()
    }

    /// Build an envelope from `(correlation, event)`, validate it, and forward
    /// it to the sink. On success returns the freshly assigned `EventId`.
    ///
    /// On failure the same `EventId` is reused for the next attempt so the
    /// recorded sequence remains contiguous; `rejected_count` is incremented
    /// and the error is returned to the caller.
    pub fn emit(
        &mut self,
        correlation: EventCorrelation,
        event: TraceEvent,
    ) -> AndromedaResult<EventId> {
        let Some(raw_id) = self.next_event_id else {
            self.rejected_count = self.rejected_count.saturating_add(1);
            return Err(observe_error(
                "EventEmitter event_id allocator is exhausted; refusing to emit",
            ));
        };
        let event_id = EventId::new(raw_id);

        let envelope = match EventEnvelope::new(event_id, correlation, event) {
            Ok(envelope) => envelope,
            Err(err) => {
                self.rejected_count = self.rejected_count.saturating_add(1);
                return Err(err);
            }
        };

        match self.sink.emit(envelope) {
            Ok(()) => {
                self.accepted_count = self.accepted_count.saturating_add(1);
                self.last_event_id = Some(event_id);
                self.next_event_id = raw_id.checked_add(1);
                Ok(event_id)
            }
            Err(err) => {
                self.rejected_count = self.rejected_count.saturating_add(1);
                Err(err)
            }
        }
    }

    /// Emit a pre-built envelope. The envelope's `event_id` must match the
    /// emitter's next allocation slot; the emitter refuses to forward
    /// out-of-band ids so monotonic ordering remains observable.
    pub fn emit_envelope(&mut self, envelope: EventEnvelope) -> AndromedaResult<EventId> {
        let Some(raw_id) = self.next_event_id else {
            self.rejected_count = self.rejected_count.saturating_add(1);
            return Err(observe_error(
                "EventEmitter event_id allocator is exhausted; refusing to emit",
            ));
        };

        if envelope.event_id.get() != raw_id {
            self.rejected_count = self.rejected_count.saturating_add(1);
            return Err(observe_error(
                "EventEmitter envelope event_id does not match the emitter's next allocation slot",
            ));
        }

        // Re-validate defensively even though `EventEnvelope::new` already
        // validated; the envelope may have been constructed elsewhere and we
        // refuse to trust its prior validation.
        if let Err(err) = envelope.validate() {
            self.rejected_count = self.rejected_count.saturating_add(1);
            return Err(err);
        }

        let event_id = envelope.event_id;
        match self.sink.emit(envelope) {
            Ok(()) => {
                self.accepted_count = self.accepted_count.saturating_add(1);
                self.last_event_id = Some(event_id);
                self.next_event_id = raw_id.checked_add(1);
                Ok(event_id)
            }
            Err(err) => {
                self.rejected_count = self.rejected_count.saturating_add(1);
                Err(err)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// In-memory query helpers
// ---------------------------------------------------------------------------

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
        self.events()
            .iter()
            .filter(|envelope| envelope.trace_id == trace_id)
            .collect()
    }

    /// All recorded envelopes whose correlation carries the given request id.
    pub fn events_for_request(&self, request_id: RequestId) -> Vec<&EventEnvelope> {
        self.events()
            .iter()
            .filter(|envelope| envelope.correlation.request_id == Some(request_id))
            .collect()
    }

    /// All recorded envelopes whose correlation carries the given session id.
    pub fn events_for_session(&self, session_id: SessionId) -> Vec<&EventEnvelope> {
        self.events()
            .iter()
            .filter(|envelope| envelope.correlation.session_id == Some(session_id))
            .collect()
    }

    /// All recorded envelopes whose correlation carries the given transaction
    /// id.
    pub fn events_for_transaction(&self, transaction_id: TransactionId) -> Vec<&EventEnvelope> {
        self.events()
            .iter()
            .filter(|envelope| envelope.correlation.transaction_id == Some(transaction_id))
            .collect()
    }

    /// All recorded envelopes whose correlation carries the given contract
    /// hash.
    pub fn events_for_contract(&self, contract_hash: ContractHash) -> Vec<&EventEnvelope> {
        self.events()
            .iter()
            .filter(|envelope| envelope.correlation.contract_hash == Some(contract_hash))
            .collect()
    }

    /// All recorded envelopes whose correlation carries the given catalog
    /// version.
    pub fn events_for_catalog_version(
        &self,
        catalog_version: CatalogVersion,
    ) -> Vec<&EventEnvelope> {
        self.events()
            .iter()
            .filter(|envelope| envelope.correlation.catalog_version == Some(catalog_version))
            .collect()
    }

    /// All recorded envelopes whose correlation carries the given catalog
    /// object id.
    pub fn events_for_catalog_object(
        &self,
        catalog_object_id: CatalogObjectId,
    ) -> Vec<&EventEnvelope> {
        self.events()
            .iter()
            .filter(|envelope| envelope.correlation.catalog_object_id == Some(catalog_object_id))
            .collect()
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
        self.events()
            .iter()
            .filter(|envelope| matches!(envelope.event, TraceEvent::TransactionTransition(_)))
            .collect()
    }

    /// All recorded envelopes whose payload is an `ExecutionTransition`
    /// trace. Mirrors [`Self::transaction_transition_events`] for the
    /// invocation-side projection.
    pub fn execution_transition_events(&self) -> Vec<&EventEnvelope> {
        self.events()
            .iter()
            .filter(|envelope| matches!(envelope.event, TraceEvent::ExecutionTransition(_)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use andromeda_core::{ContractHash, RequestId, SessionId, TransactionId};

    use super::*;
    use crate::events::{
        BackpressureTrace, CompletionEmittedTrace, EventCorrelation, ProtocolCorrelation,
        ProtocolEventScope, TraceEvent,
    };

    fn backpressure_event(trace: u128) -> TraceEvent {
        TraceEvent::Backpressure(BackpressureTrace {
            trace_id: TraceId::new(trace),
            scope: ProtocolEventScope::Connection,
            protocol: ProtocolCorrelation::empty(),
            retry_after_micros: Some(100),
            pending_units: None,
            limit_units: None,
            reason: "bounded queue is full".to_string(),
        })
    }

    fn completion_event(trace: u128) -> TraceEvent {
        TraceEvent::CompletionEmitted(CompletionEmittedTrace {
            trace_id: TraceId::new(trace),
            protocol: ProtocolCorrelation::empty(),
            completion_code: Some(1),
            committed: false,
            durable_lsn: None,
            reason: "request rejected by precondition".to_string(),
        })
    }

    #[test]
    fn emitter_assigns_monotonic_event_ids_on_success() {
        let mut emitter = EventEmitter::new(InMemoryEventSink::new());

        let id_1 = emitter
            .emit(EventCorrelation::empty(), backpressure_event(11))
            .unwrap();
        let id_2 = emitter
            .emit(EventCorrelation::empty(), backpressure_event(12))
            .unwrap();
        let id_3 = emitter
            .emit(EventCorrelation::empty(), backpressure_event(13))
            .unwrap();

        assert_eq!(id_1.get(), 1);
        assert_eq!(id_2.get(), 2);
        assert_eq!(id_3.get(), 3);
        assert_eq!(emitter.accepted_count(), 3);
        assert_eq!(emitter.rejected_count(), 0);
        assert_eq!(emitter.last_event_id(), Some(id_3));
        assert_eq!(emitter.sink().len(), 3);
    }

    #[test]
    fn emitter_surfaces_validation_errors_and_does_not_burn_event_id() {
        let mut emitter = EventEmitter::new(InMemoryEventSink::new());

        // Invalid event: zero trace id is rejected by EventEnvelope::validate.
        let err = emitter
            .emit(EventCorrelation::empty(), backpressure_event(0))
            .unwrap_err();
        assert!(err.message().contains("trace_id must be non-zero"));
        assert_eq!(emitter.rejected_count(), 1);
        assert_eq!(emitter.accepted_count(), 0);
        assert_eq!(emitter.sink().len(), 0);

        // Next valid emit must reuse event_id 1 (no silent gap).
        let id = emitter
            .emit(EventCorrelation::empty(), backpressure_event(7))
            .unwrap();
        assert_eq!(id.get(), 1);
        assert_eq!(emitter.accepted_count(), 1);
        assert_eq!(emitter.rejected_count(), 1);
    }

    #[test]
    fn emitter_propagates_sink_failures_without_silent_success() {
        struct AlwaysFailingSink;
        impl EventSink for AlwaysFailingSink {
            fn emit(&mut self, _event: EventEnvelope) -> AndromedaResult<()> {
                Err(observe_error("sink unavailable"))
            }
        }

        let mut emitter = EventEmitter::new(AlwaysFailingSink);
        let err = emitter
            .emit(EventCorrelation::empty(), backpressure_event(9))
            .unwrap_err();
        assert_eq!(err.message(), "sink unavailable");
        assert_eq!(emitter.accepted_count(), 0);
        assert_eq!(emitter.rejected_count(), 1);
        assert_eq!(emitter.last_event_id(), None);
        // Failed sink writes also reuse the slot so the next attempt re-tries
        // event_id 1 rather than fabricating a phantom gap.
        assert_eq!(emitter.peek_next_event_id().map(EventId::get), Some(1));
    }

    #[test]
    fn emitter_rejects_zero_starting_event_id() {
        let err = EventEmitter::with_starting_event_id(InMemoryEventSink::new(), 0).unwrap_err();
        assert!(err.message().contains("non-zero"));
    }

    #[test]
    fn emitter_envelope_event_id_must_match_allocator_slot() {
        let mut emitter =
            EventEmitter::with_starting_event_id(InMemoryEventSink::new(), 100).unwrap();

        let envelope = EventEnvelope::new(
            EventId::new(200),
            EventCorrelation::empty(),
            backpressure_event(5),
        )
        .unwrap();

        let err = emitter.emit_envelope(envelope).unwrap_err();
        assert!(err.message().contains("does not match"));
        assert_eq!(emitter.rejected_count(), 1);
        assert_eq!(emitter.sink().len(), 0);

        // A correctly numbered envelope succeeds and advances the allocator.
        let envelope = EventEnvelope::new(
            EventId::new(100),
            EventCorrelation::empty(),
            backpressure_event(5),
        )
        .unwrap();
        let id = emitter.emit_envelope(envelope).unwrap();
        assert_eq!(id.get(), 100);
        assert_eq!(emitter.accepted_count(), 1);
        assert_eq!(emitter.peek_next_event_id().map(EventId::get), Some(101));
    }

    #[test]
    fn emitter_marks_id_space_as_exhausted_after_max() {
        let mut emitter =
            EventEmitter::with_starting_event_id(InMemoryEventSink::new(), u128::MAX).unwrap();

        let _ = emitter
            .emit(EventCorrelation::empty(), backpressure_event(1))
            .unwrap();

        assert!(emitter.is_exhausted());
        let err = emitter
            .emit(EventCorrelation::empty(), backpressure_event(2))
            .unwrap_err();
        assert!(err.message().contains("exhausted"));
        assert_eq!(emitter.accepted_count(), 1);
        assert_eq!(emitter.rejected_count(), 1);
    }

    #[test]
    fn in_memory_sink_query_helpers_pivot_by_correlation() {
        let mut emitter = EventEmitter::new(InMemoryEventSink::new());

        let request_a = RequestId::new(1001);
        let session_a = SessionId::new(2001);
        let session_b = SessionId::new(2002);
        let tx_a = TransactionId::new(3001);
        let contract = ContractHash::test_vector(7);

        let mut corr_a = EventCorrelation::empty();
        corr_a.request_id = Some(request_a);
        corr_a.session_id = Some(session_a);
        corr_a.contract_hash = Some(contract);

        let mut corr_b = EventCorrelation::empty();
        corr_b.session_id = Some(session_b);
        corr_b.transaction_id = Some(tx_a);

        let id_a = emitter.emit(corr_a, backpressure_event(101)).unwrap();
        let id_b = emitter.emit(corr_b, backpressure_event(102)).unwrap();
        let id_c = emitter.emit(corr_a, completion_event(101)).unwrap();

        let sink = emitter.sink();
        assert_eq!(sink.len(), 3);
        assert!(!sink.is_empty());

        let by_trace = sink.events_for_trace(TraceId::new(101));
        assert_eq!(by_trace.len(), 2);
        assert!(by_trace.iter().any(|e| e.event_id == id_a));
        assert!(by_trace.iter().any(|e| e.event_id == id_c));

        let by_request = sink.events_for_request(request_a);
        assert_eq!(by_request.len(), 2);

        let by_session_a = sink.events_for_session(session_a);
        assert_eq!(by_session_a.len(), 2);
        let by_session_b = sink.events_for_session(session_b);
        assert_eq!(by_session_b.len(), 1);
        assert_eq!(by_session_b[0].event_id, id_b);

        let by_tx = sink.events_for_transaction(tx_a);
        assert_eq!(by_tx.len(), 1);
        assert_eq!(by_tx[0].event_id, id_b);

        let by_contract = sink.events_for_contract(contract);
        assert_eq!(by_contract.len(), 2);

        assert_eq!(sink.find_event(id_a).map(|e| e.event_id), Some(id_a));
        assert!(sink.find_event(EventId::new(9999)).is_none());
    }

    #[test]
    fn into_sink_yields_recorded_envelopes() {
        let mut emitter = EventEmitter::new(InMemoryEventSink::new());
        let _ = emitter
            .emit(EventCorrelation::empty(), backpressure_event(1))
            .unwrap();
        let _ = emitter
            .emit(EventCorrelation::empty(), backpressure_event(2))
            .unwrap();

        let sink = emitter.into_sink();
        let events = sink.into_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_id.get(), 1);
        assert_eq!(events[1].event_id.get(), 2);
    }

    #[test]
    fn emitter_routes_transition_traces_into_sink_with_query_helpers() {
        use crate::events::{
            ExecutionTransitionTrace, TransactionPhaseCode, TransactionTransitionTrace,
            TransitionReasonCode,
        };
        use andromeda_core::InvocationId;

        let mut emitter = EventEmitter::new(InMemoryEventSink::new());

        let request = RequestId::new(11);
        let session = SessionId::new(12);
        let transaction = TransactionId::new(101);

        let mut corr = EventCorrelation::empty();
        corr.request_id = Some(request);
        corr.session_id = Some(session);
        corr.transaction_id = Some(transaction);
        corr.durable_lsn = Some(2024);

        let tx_event = TraceEvent::TransactionTransition(TransactionTransitionTrace {
            trace_id: TraceId::new(701),
            transaction_id: transaction,
            invocation_id: Some(InvocationId::new(40)),
            request_id: Some(request),
            session_id: Some(session),
            prev_phase: TransactionPhaseCode::COMMITTING,
            next_phase: TransactionPhaseCode::COMMITTED,
            durable_lsn: Some(2024),
            reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
            reason: "WAL flush proved durable commit".to_string(),
        });
        let exec_event = TraceEvent::ExecutionTransition(ExecutionTransitionTrace {
            trace_id: TraceId::new(702),
            invocation_id: InvocationId::new(40),
            request_id: Some(request),
            session_id: Some(session),
            transaction_id: Some(transaction),
            completion_code: Some(1),
            prev_phase: Some(TransactionPhaseCode::COMMITTING),
            next_phase: Some(TransactionPhaseCode::COMMITTED),
            durable_lsn: Some(2024),
            reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
            reason: "executor observed durable commit".to_string(),
        });

        let id_tx = emitter
            .emit(corr, tx_event)
            .expect("tx transition accepted");
        let id_exec = emitter
            .emit(corr, exec_event)
            .expect("exec transition accepted");

        let sink = emitter.sink();
        assert_eq!(sink.transaction_transition_events().len(), 1);
        assert_eq!(sink.execution_transition_events().len(), 1);
        let by_tx = sink.events_for_transaction(transaction);
        assert_eq!(by_tx.len(), 2);
        assert!(by_tx.iter().any(|e| e.event_id == id_tx));
        assert!(by_tx.iter().any(|e| e.event_id == id_exec));
    }
}
