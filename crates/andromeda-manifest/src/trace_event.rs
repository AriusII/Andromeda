//! Manifest switch trace event — advisory observability for the commit path.
//!
//! [`ManifestSwitchEvent`] is emitted whenever the active manifest is switched
//! to a new generation.  The event is advisory: if the receiver is
//! disconnected or the channel is full the event is silently dropped and the
//! critical commit / recovery path is never blocked.
//!
//! ## Usage
//!
//! ```rust
//! use andromeda_manifest::trace_event::{ManifestSwitchEvent, ManifestSwitchTracer};
//! use andromeda_wal::Lsn;
//!
//! let (tracer, receiver) = ManifestSwitchTracer::new_pair();
//!
//! tracer.emit(ManifestSwitchEvent::new(
//!     2,          // generation
//!     1,          // old_manifest_version
//!     2,          // new_manifest_version
//!     0,          // timestamp_nanos (caller-supplied; advisory)
//!     Lsn::new(500), // durable_lsn_at_switch
//! ));
//!
//! let event = receiver.recv().unwrap();
//! assert_eq!(event.new_manifest_version, 2);
//! ```

#![forbid(unsafe_code)]

use std::sync::mpsc::{Receiver, SyncSender};

use andromeda_wal::Lsn;

// ─── Event type ───────────────────────────────────────────────────────────────

/// A trace event emitted when the active database manifest switches to a new
/// generation.
///
/// All fields are advisory.  This event must never be on the commit/recovery
/// critical path — failure to deliver it must be silently ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestSwitchEvent {
    /// Root pointer generation of the new manifest (monotonically increasing).
    pub generation: u64,
    /// Manifest version of the outgoing manifest (`0` if no prior manifest).
    pub old_manifest_version: u64,
    /// Manifest version of the incoming manifest.
    pub new_manifest_version: u64,
    /// Caller-supplied wall-clock timestamp in nanoseconds since Unix epoch.
    /// Advisory; not used for correctness decisions.
    pub timestamp_nanos: u64,
    /// Durable WAL LSN at the instant the switch fence passed.
    /// Must satisfy `durable_lsn_at_switch >= new_manifest.base_checkpoint_lsn`.
    pub durable_lsn_at_switch: Lsn,
}

impl ManifestSwitchEvent {
    /// Construct a new [`ManifestSwitchEvent`].
    #[must_use]
    pub fn new(
        generation: u64,
        old_manifest_version: u64,
        new_manifest_version: u64,
        timestamp_nanos: u64,
        durable_lsn_at_switch: Lsn,
    ) -> Self {
        Self {
            generation,
            old_manifest_version,
            new_manifest_version,
            timestamp_nanos,
            durable_lsn_at_switch,
        }
    }
}

// ─── Tracer ───────────────────────────────────────────────────────────────────

/// Advisory trace channel for [`ManifestSwitchEvent`] emissions.
///
/// Backed by a bounded `std::sync::mpsc::SyncSender`.  Dropped events (channel
/// full or receiver disconnected) do **not** produce errors — the commit path
/// must never block on observability.
#[derive(Debug, Clone)]
pub struct ManifestSwitchTracer {
    sender: SyncSender<ManifestSwitchEvent>,
}

impl ManifestSwitchTracer {
    /// Capacity of the trace channel's internal ring buffer.
    pub const CHANNEL_CAPACITY: usize = 256;

    /// Create a linked tracer/receiver pair.
    ///
    /// The tracer can be cloned and shared across threads; the receiver is
    /// single-consumer.
    #[must_use]
    pub fn new_pair() -> (Self, Receiver<ManifestSwitchEvent>) {
        let (sender, receiver) = std::sync::mpsc::sync_channel(Self::CHANNEL_CAPACITY);
        (Self { sender }, receiver)
    }

    /// Emit a [`ManifestSwitchEvent`] to the trace channel.
    ///
    /// If the channel is full or the receiver has been dropped, the event is
    /// silently discarded.  This method never blocks.
    pub fn emit(&self, event: ManifestSwitchEvent) {
        // `try_send` returns Err on Full or Disconnected — both are acceptable;
        // observability must never impede the critical path.
        let _ = self.sender.try_send(event);
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_wal::Lsn;

    fn sample_event(generation: u64) -> ManifestSwitchEvent {
        ManifestSwitchEvent::new(
            generation,
            generation.saturating_sub(1),
            generation,
            1_700_000_000_000_000_000,
            Lsn::new(generation * 100),
        )
    }

    #[test]
    fn tracer_emits_and_receiver_receives_event() {
        let (tracer, rx) = ManifestSwitchTracer::new_pair();
        tracer.emit(sample_event(3));
        let event = rx.try_recv().expect("event must be present");
        assert_eq!(event.generation, 3);
        assert_eq!(event.new_manifest_version, 3);
        assert_eq!(event.durable_lsn_at_switch, Lsn::new(300));
    }

    #[test]
    fn tracer_emit_does_not_block_on_disconnected_receiver() {
        let (tracer, rx) = ManifestSwitchTracer::new_pair();
        drop(rx); // disconnect receiver
        // Must not panic or block
        tracer.emit(sample_event(1));
    }

    #[test]
    fn tracer_emit_drops_events_silently_when_channel_full() {
        let (tracer, _rx) = ManifestSwitchTracer::new_pair();
        // Fill the channel beyond capacity
        for i in 0..ManifestSwitchTracer::CHANNEL_CAPACITY + 10 {
            tracer.emit(sample_event(i as u64));
        }
        // No panic — overflow is silently dropped
    }

    #[test]
    fn tracer_preserves_all_event_fields() {
        let (tracer, rx) = ManifestSwitchTracer::new_pair();
        let original = ManifestSwitchEvent::new(7, 6, 7, 42_000_000_000, Lsn::new(999));
        tracer.emit(original.clone());
        let received = rx.try_recv().unwrap();
        assert_eq!(received, original);
    }

    #[test]
    fn tracer_can_be_cloned_and_sends_from_multiple_handles() {
        let (tracer_a, rx) = ManifestSwitchTracer::new_pair();
        let tracer_b = tracer_a.clone();
        tracer_a.emit(sample_event(1));
        tracer_b.emit(sample_event(2));
        let e1 = rx.try_recv().unwrap();
        let e2 = rx.try_recv().unwrap();
        // Order is FIFO
        assert_eq!(e1.generation, 1);
        assert_eq!(e2.generation, 2);
    }
}
