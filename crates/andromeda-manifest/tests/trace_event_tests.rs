//! Integration tests for [`ManifestSwitchEvent`] emission and delivery.
//!
//! Validates that:
//! - Events are correctly emitted via [`ManifestSwitchTracer::emit`].
//! - All event fields survive the channel round-trip without mutation.
//! - The tracer never blocks or panics when the channel is full or the receiver
//!   has been dropped.
//! - The event traces a correct manifest switch: generation advances, LSN is
//!   durable, and old/new versions are recorded.

#![forbid(unsafe_code)]

use andromeda_manifest::{ManifestSwitchEvent, ManifestSwitchTracer};
use andromeda_wal::Lsn;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn switch_event(
    generation: u64,
    old_version: u64,
    new_version: u64,
    durable_lsn: u64,
) -> ManifestSwitchEvent {
    ManifestSwitchEvent::new(
        generation,
        old_version,
        new_version,
        1_700_000_000_000_000_000_u64.wrapping_add(generation),
        Lsn::new(durable_lsn),
    )
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// TE-01: A single emit is received with all fields intact.
#[test]
fn te01_single_event_emission_and_reception() {
    let (tracer, rx) = ManifestSwitchTracer::new_pair();
    let event = switch_event(1, 0, 1, 500);
    tracer.emit(event.clone());

    let received = rx.try_recv().expect("event must be in channel");
    assert_eq!(received, event, "received event must match emitted event");
}

/// TE-02: All fields are preserved across the channel.
#[test]
fn te02_all_event_fields_preserved() {
    let (tracer, rx) = ManifestSwitchTracer::new_pair();
    let event = ManifestSwitchEvent {
        generation: 7,
        old_manifest_version: 6,
        new_manifest_version: 7,
        timestamp_nanos: 1_234_567_890_000_000_000,
        durable_lsn_at_switch: Lsn::new(99_999),
    };
    tracer.emit(event.clone());

    let r = rx.try_recv().unwrap();
    assert_eq!(r.generation, 7);
    assert_eq!(r.old_manifest_version, 6);
    assert_eq!(r.new_manifest_version, 7);
    assert_eq!(r.timestamp_nanos, 1_234_567_890_000_000_000);
    assert_eq!(r.durable_lsn_at_switch, Lsn::new(99_999));
}

/// TE-03: Emit does not block or panic when the receiver has been dropped.
#[test]
fn te03_emit_does_not_panic_on_disconnected_receiver() {
    let (tracer, rx) = ManifestSwitchTracer::new_pair();
    drop(rx);
    // Must not panic or block
    tracer.emit(switch_event(1, 0, 1, 100));
    tracer.emit(switch_event(2, 1, 2, 200));
}

/// TE-04: Events emitted in order are received in FIFO order.
#[test]
fn te04_events_received_in_fifo_order() {
    let (tracer, rx) = ManifestSwitchTracer::new_pair();
    for g in 1u64..=5 {
        tracer.emit(switch_event(g, g - 1, g, g * 100));
    }
    for expected_gen in 1u64..=5 {
        let r = rx.try_recv().expect("event must be present");
        assert_eq!(r.generation, expected_gen);
    }
    assert!(
        rx.try_recv().is_err(),
        "channel must be empty after 5 events"
    );
}

/// TE-05: When channel is saturated, additional events are silently dropped.
#[test]
fn te05_channel_overflow_silently_drops_excess_events() {
    let (tracer, _rx) = ManifestSwitchTracer::new_pair();
    let overflow_count = ManifestSwitchTracer::CHANNEL_CAPACITY + 50;
    for g in 0..overflow_count as u64 {
        tracer.emit(switch_event(g, g.saturating_sub(1), g, g * 10));
    }
    // No panic — overflow events are silently dropped
}

/// TE-06: Tracer can be cloned; both handles send to the same receiver.
#[test]
fn te06_cloned_tracers_share_channel() {
    let (tracer_a, rx) = ManifestSwitchTracer::new_pair();
    let tracer_b = tracer_a.clone();

    tracer_a.emit(switch_event(1, 0, 1, 100));
    tracer_b.emit(switch_event(2, 1, 2, 200));

    let e1 = rx.try_recv().unwrap();
    let e2 = rx.try_recv().unwrap();
    assert_eq!(e1.generation, 1);
    assert_eq!(e2.generation, 2);
}

/// TE-07: A manifest switch event correctly models the transition
/// from generation N to N+1 with durable LSN coverage.
///
/// This is the key invariant: `durable_lsn_at_switch` must be ≥
/// the new manifest's `base_checkpoint_lsn`.  We verify the event
/// carries enough information to evaluate this invariant.
#[test]
fn te07_manifest_switch_event_carries_durability_evidence() {
    let (tracer, rx) = ManifestSwitchTracer::new_pair();

    // Simulate: old manifest v1 (base_ckpt=500), new manifest v2 (base_ckpt=1000),
    // WAL durable at 1200 — fence should have passed.
    let durable_lsn = Lsn::new(1200);
    let new_base_checkpoint_lsn = Lsn::new(1000);

    let event = ManifestSwitchEvent::new(2, 1, 2, 0, durable_lsn);
    tracer.emit(event);

    let received = rx.try_recv().unwrap();
    assert!(
        received.durable_lsn_at_switch >= new_base_checkpoint_lsn,
        "durable LSN ({}) must cover new manifest base checkpoint ({})",
        received.durable_lsn_at_switch.get(),
        new_base_checkpoint_lsn.get()
    );
    assert_eq!(received.generation, 2);
    assert_eq!(received.old_manifest_version, 1);
    assert_eq!(received.new_manifest_version, 2);
}
