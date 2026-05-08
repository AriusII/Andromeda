use andromeda_core::{AndromedaError, AndromedaResult};

use crate::events::{
    EventCorrelation, EventEnvelope, EventId, EventSink, TraceEvent, observe_error,
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

    fn record_rejection(&mut self) {
        self.rejected_count = self.rejected_count.saturating_add(1);
    }

    fn reject<T>(&mut self, err: AndromedaError) -> AndromedaResult<T> {
        self.record_rejection();
        Err(err)
    }

    fn next_raw_event_id(&mut self) -> AndromedaResult<u128> {
        match self.next_event_id {
            Some(raw_id) => Ok(raw_id),
            None => self.reject(observe_error(
                "EventEmitter event_id allocator is exhausted; refusing to emit",
            )),
        }
    }

    fn record_acceptance(&mut self, event_id: EventId, raw_id: u128) {
        self.accepted_count = self.accepted_count.saturating_add(1);
        self.last_event_id = Some(event_id);
        self.next_event_id = raw_id.checked_add(1);
    }

    fn emit_to_sink(&mut self, envelope: EventEnvelope, raw_id: u128) -> AndromedaResult<EventId> {
        let event_id = envelope.event_id;
        match self.sink.emit(envelope) {
            Ok(()) => {
                self.record_acceptance(event_id, raw_id);
                Ok(event_id)
            }
            Err(err) => self.reject(err),
        }
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
        let raw_id = self.next_raw_event_id()?;
        let event_id = EventId::new(raw_id);

        let envelope = match EventEnvelope::new(event_id, correlation, event) {
            Ok(envelope) => envelope,
            Err(err) => return self.reject(err),
        };

        self.emit_to_sink(envelope, raw_id)
    }

    /// Emit a pre-built envelope. The envelope's `event_id` must match the
    /// emitter's next allocation slot; the emitter refuses to forward
    /// out-of-band ids so monotonic ordering remains observable.
    pub fn emit_envelope(&mut self, envelope: EventEnvelope) -> AndromedaResult<EventId> {
        let raw_id = self.next_raw_event_id()?;

        if envelope.event_id.get() != raw_id {
            return self.reject(observe_error(
                "EventEmitter envelope event_id does not match the emitter's next allocation slot",
            ));
        }

        // Re-validate defensively even though `EventEnvelope::new` already
        // validated; the envelope may have been constructed elsewhere and we
        // refuse to trust its prior validation.
        if let Err(err) = envelope.validate() {
            return self.reject(err);
        }

        self.emit_to_sink(envelope, raw_id)
    }
}
