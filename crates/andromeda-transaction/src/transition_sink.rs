//! Pluggable sink for [`TransactionTransitionTrace`] emission.
//!
//! Every successful state-machine transition emits one trace record into the
//! configured sink. The sink is **observability-only** and is never consulted
//! for commit visibility or durable WAL gating; those invariants remain owned
//! by [`crate::TransactionStateMachine`].
//!
//! # Trait objects
//!
//! [`TransactionTransitionSink`] is object-safe and `Send + Sync`, so
//! [`TransactionManager`][crate::TransactionManager] stores it as
//! `Arc<dyn TransactionTransitionSink>`.
//!
//! # Built-in implementations
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`NullTransitionSink`] | No-op default; zero overhead |
//! | [`InMemoryTransitionSink`] | Test helper; collects traces in memory |

use std::sync::Mutex;

use andromeda_observability::TransactionTransitionTrace;

/// Pluggable receiver for state-machine transition traces.
///
/// Implementations must be `Send + Sync` because [`TransactionManager`] may be
/// driven concurrently from multiple threads.
///
/// # Contract
///
/// - `record` is called **only after** a state-machine transition has
///   succeeded; sinks must never treat a call as a commit or rollback signal.
/// - Sinks must not panic; a panicking sink will abort the emission silently
///   (the transition itself has already completed at call time).
/// - No re-entrancy into [`TransactionManager`] from inside `record` is
///   permitted; doing so will deadlock.
///
/// [`TransactionManager`]: crate::TransactionManager
pub trait TransactionTransitionSink: Send + Sync {
    /// Record a single transition trace produced by a successful state-machine
    /// transition.
    fn record(&self, trace: TransactionTransitionTrace);
}

// ---------------------------------------------------------------------------
// NullTransitionSink
// ---------------------------------------------------------------------------

/// No-op [`TransactionTransitionSink`] used as the default when no explicit
/// sink is configured.
///
/// All calls are silently discarded. There is no allocation overhead.
pub struct NullTransitionSink;

impl TransactionTransitionSink for NullTransitionSink {
    #[inline]
    fn record(&self, _trace: TransactionTransitionTrace) {}
}

// ---------------------------------------------------------------------------
// InMemoryTransitionSink
// ---------------------------------------------------------------------------

/// In-memory [`TransactionTransitionSink`] that collects transition traces for
/// test assertions.
///
/// Traces are appended in emission order. Use [`drain`][Self::drain] to
/// consume all recorded traces (clearing the buffer) or
/// [`snapshot`][Self::snapshot] to clone them non-destructively.
///
/// # Thread safety
///
/// The internal buffer is protected by a `Mutex`; the sink can be shared
/// across threads and satisfies the `Send + Sync` bound of
/// [`TransactionTransitionSink`].
pub struct InMemoryTransitionSink {
    records: Mutex<Vec<TransactionTransitionTrace>>,
}

impl InMemoryTransitionSink {
    /// Construct an empty sink.
    pub fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
        }
    }

    /// Remove and return all traces recorded so far, clearing the buffer.
    pub fn drain(&self) -> Vec<TransactionTransitionTrace> {
        self.records
            .lock()
            .expect("InMemoryTransitionSink mutex poisoned")
            .drain(..)
            .collect()
    }

    /// Return a clone of all recorded traces without removing them.
    pub fn snapshot(&self) -> Vec<TransactionTransitionTrace> {
        self.records
            .lock()
            .expect("InMemoryTransitionSink mutex poisoned")
            .clone()
    }

    /// Number of traces recorded so far.
    pub fn len(&self) -> usize {
        self.records
            .lock()
            .expect("InMemoryTransitionSink mutex poisoned")
            .len()
    }

    /// Returns `true` if no traces have been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for InMemoryTransitionSink {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionTransitionSink for InMemoryTransitionSink {
    fn record(&self, trace: TransactionTransitionTrace) {
        self.records
            .lock()
            .expect("InMemoryTransitionSink mutex poisoned")
            .push(trace);
    }
}

#[cfg(test)]
mod tests {
    use andromeda_observability::{
        TraceId, TransactionPhaseCode, TransactionTransitionTrace, TransitionReasonCode,
    };
    use andromeda_types::TransactionId;

    use super::*;

    fn make_trace(id: u128) -> TransactionTransitionTrace {
        TransactionTransitionTrace {
            trace_id: TraceId::new(id),
            transaction_id: TransactionId::new(id as u64),
            invocation_id: None,
            request_id: None,
            session_id: None,
            prev_phase: TransactionPhaseCode::ACTIVE,
            next_phase: TransactionPhaseCode::COMMITTING,
            durable_lsn: None,
            reason_code: TransitionReasonCode::NORMAL_PROGRESS,
            reason: "test".to_string(),
        }
    }

    #[test]
    fn null_sink_accepts_without_panic() {
        let sink = NullTransitionSink;
        sink.record(make_trace(1));
        sink.record(make_trace(2));
        // no assertion needed — just must not panic
    }

    #[test]
    fn in_memory_sink_collects_in_order() {
        let sink = InMemoryTransitionSink::new();
        assert!(sink.is_empty());

        sink.record(make_trace(10));
        sink.record(make_trace(20));
        assert_eq!(sink.len(), 2);

        let snapped = sink.snapshot();
        assert_eq!(snapped.len(), 2);
        assert_eq!(snapped[0].trace_id, TraceId::new(10));
        assert_eq!(snapped[1].trace_id, TraceId::new(20));

        // snapshot is non-destructive
        assert_eq!(sink.len(), 2);

        let drained = sink.drain();
        assert_eq!(drained.len(), 2);
        assert!(sink.is_empty());
    }
}
