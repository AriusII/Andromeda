/// W3 — MapDeltaLog ordering, idempotency, and crash-replay contract tests.
///
/// Covers Invariants 1, 4, and 6 from A4 audit:
///   - Invariant 1: ordering by source LSN must be strictly increasing.
///   - Invariant 4: idempotent application — replay of the same delta is a no-op.
///   - Invariant 6: crash mid-application — recovery returns to last consistent state.
use std::num::NonZeroU64;

use andromeda_maps::{
    MapDeltaApplyOutcome, MapDeltaApplyState, MapDeltaLog, MapDescriptor, MapDescriptorError,
    MapGrain, MapId, MapPublicationEvidence, MapPublicationRecoveryEvidence, MapRefreshMode,
    MapStalenessPolicy,
};

fn incremental_descriptor() -> MapDescriptor {
    MapDescriptor::new(
        MapId::new(101).unwrap(),
        MapGrain::Partition,
        MapRefreshMode::Incremental,
        MapStalenessPolicy::bounded_millis(500).unwrap(),
    )
}

fn non_empty_digest(seed: u8) -> [u8; 32] {
    let mut d = [0u8; 32];
    d[0] = seed;
    d
}

// ─── Invariant 1 ─────────────────────────────────────────────────────────────

/// `MapDeltaLog::new` MUST reject a delta window where `delta_lsn <= base_snapshot_lsn`.
///
/// This proves that a temporally inverted (or zero-length) delta record cannot be
/// constructed and can therefore never reach the admission path.
#[test]
fn map_delta_log_rejects_non_monotonic_source_lsn() {
    let descriptor = incremental_descriptor();
    let base = NonZeroU64::new(100).unwrap();
    let table_records = NonZeroU64::new(4).unwrap();
    let map_records = NonZeroU64::new(2).unwrap();

    // Case 1: delta_lsn strictly less than base_snapshot_lsn (inverted window).
    let err = MapDeltaLog::new(
        descriptor,
        base,
        NonZeroU64::new(5).unwrap(), // 5 < 100
        table_records,
        map_records,
    )
    .unwrap_err();
    assert_eq!(
        err,
        MapDescriptorError::InvertedDeltaLsnWindow,
        "inverted window (delta < base) must be rejected"
    );

    // Case 2: delta_lsn equal to base_snapshot_lsn (zero-length window, not strictly greater).
    let err_equal = MapDeltaLog::new(
        descriptor,
        base,
        base, // 100 == 100, not strictly greater
        table_records,
        map_records,
    )
    .unwrap_err();
    assert_eq!(
        err_equal,
        MapDescriptorError::InvertedDeltaLsnWindow,
        "zero-length window (delta == base) must also be rejected"
    );

    // Case 3: valid forward window is accepted.
    let valid = MapDeltaLog::new(
        descriptor,
        base,
        NonZeroU64::new(101).unwrap(), // 101 > 100 ✓
        table_records,
        map_records,
    );
    assert!(valid.is_ok(), "forward window must be accepted");
    let log = valid.unwrap();
    assert_eq!(log.base_snapshot_lsn.get(), 100);
    assert_eq!(log.delta_lsn.get(), 101);
}

// ─── Invariant 4 ─────────────────────────────────────────────────────────────

/// Applying the same delta log a second time via `MapDeltaApplyState::try_apply`
/// MUST return `AlreadyApplied` rather than advancing the cursor again.
///
/// This proves the idempotency guard: a replay (e.g., after a WAL re-scan or a
/// network retry) cannot double-count WAL records or produce a duplicate
/// `MapRefreshAdmissionDecision`.
#[test]
fn map_delta_log_replay_is_idempotent() {
    let descriptor = incremental_descriptor();
    let delta_log = MapDeltaLog::new(
        descriptor,
        NonZeroU64::new(50).unwrap(),
        NonZeroU64::new(60).unwrap(),
        NonZeroU64::new(3).unwrap(),
        NonZeroU64::new(1).unwrap(),
    )
    .unwrap();

    let mut state = MapDeltaApplyState::new(descriptor);

    // First application: cursor advances to delta_lsn = 60.
    let first = state.try_apply(delta_log).unwrap();
    assert_eq!(
        first,
        MapDeltaApplyOutcome::Applied {
            new_applied_up_to_lsn: NonZeroU64::new(60).unwrap(),
        },
        "first apply must advance the cursor"
    );
    assert_eq!(
        state.applied_up_to_lsn,
        Some(NonZeroU64::new(60).unwrap()),
        "apply state must reflect new frontier"
    );

    // Second application of the SAME delta: idempotent ack.
    let second = state.try_apply(delta_log).unwrap();
    assert_eq!(
        second,
        MapDeltaApplyOutcome::AlreadyApplied,
        "second apply of same delta must be a no-op idempotent ack"
    );

    // Cursor must NOT have moved.
    assert_eq!(
        state.applied_up_to_lsn,
        Some(NonZeroU64::new(60).unwrap()),
        "cursor must remain at 60 after idempotent replay"
    );

    // A subsequent NEW delta with delta_lsn = 70 is still applied correctly.
    let next_delta = MapDeltaLog::new(
        descriptor,
        NonZeroU64::new(60).unwrap(),
        NonZeroU64::new(70).unwrap(),
        NonZeroU64::new(5).unwrap(),
        NonZeroU64::new(2).unwrap(),
    )
    .unwrap();
    let third = state.try_apply(next_delta).unwrap();
    assert_eq!(
        third,
        MapDeltaApplyOutcome::Applied {
            new_applied_up_to_lsn: NonZeroU64::new(70).unwrap(),
        },
        "fresh delta after idempotent replay must advance the cursor"
    );
}

// ─── Invariant 6 ─────────────────────────────────────────────────────────────

/// Simulates a crash mid-incremental-application and proves that recovery returns
/// to the last durably-committed publication state — not a partially-applied state.
///
/// Crash simulation protocol:
/// 1. Establish a durable `MapPublicationEvidence` at LSN 50 (last committed).
/// 2. Begin applying a delta (LSN window 50→60).  The apply state advances in memory.
/// 3. Crash: the in-flight `MapDeltaApplyState` is discarded (never persisted).
/// 4. Recovery: wrap the durable publication in `MapPublicationRecoveryEvidence`.
/// 5. Assert recovered publication is at LSN 50 and is never source truth.
/// 6. Assert a fresh `MapDeltaApplyState` seeded from the recovered frontier can
///    safely re-apply the same delta — the result is `Applied` (not `AlreadyApplied`),
///    because the apply cursor was lost in the crash.
#[test]
fn map_delta_log_crash_mid_application_is_recoverable_to_consistent_state() {
    let descriptor = incremental_descriptor();
    let digest = non_empty_digest(0xAB);
    let recovery_digest = non_empty_digest(0xCD);

    // ── Step 1: last durable publication at LSN 50 ──────────────────────────
    let durable_publication = MapPublicationEvidence::new(descriptor, 1, 1, 50, digest).unwrap();

    assert_eq!(durable_publication.publication_lsn, 50);
    assert!(
        !durable_publication.is_source_truth(),
        "Map must never be source truth"
    );

    // ── Step 2: in-flight apply state — advances cursor to 60 ───────────────
    let delta_50_to_60 = MapDeltaLog::new(
        descriptor,
        NonZeroU64::new(50).unwrap(),
        NonZeroU64::new(60).unwrap(),
        NonZeroU64::new(7).unwrap(),
        NonZeroU64::new(3).unwrap(),
    )
    .unwrap();

    let mut in_flight_state = MapDeltaApplyState::new(descriptor);
    let apply_result = in_flight_state.try_apply(delta_50_to_60).unwrap();
    assert_eq!(
        apply_result,
        MapDeltaApplyOutcome::Applied {
            new_applied_up_to_lsn: NonZeroU64::new(60).unwrap(),
        },
        "in-flight application should succeed before the crash"
    );

    // ── Step 3: crash — in_flight_state is dropped (never committed) ────────
    let _ = in_flight_state;
    // (Variable is gone; in a real engine this is the process re-start boundary.)

    // ── Step 4: recovery wraps the last durable publication ─────────────────
    let recovery_evidence = MapPublicationRecoveryEvidence::new(
        durable_publication,
        55, // recovery_lsn: the WAL position at which recovery ran
        recovery_digest,
    )
    .unwrap();

    // ── Step 5: recovered state must match last durable publication ──────────
    let recovered_active = recovery_evidence.active_after_recovery();
    assert_eq!(
        recovered_active.publication_lsn, 50,
        "recovery must restore the last durably-committed publication, not the crashed state"
    );
    assert!(
        !recovery_evidence.is_source_truth(),
        "recovered map evidence must never be source truth"
    );

    // ── Step 6: re-apply the same delta from recovered frontier ─────────────
    // The apply cursor was lost in the crash.  Seeding from publication_lsn=50
    // means the cursor starts at 50.  Re-applying delta (50→60) is a *fresh*
    // Apply from the perspective of this session.
    let recovered_lsn = NonZeroU64::new(recovered_active.publication_lsn)
        .expect("publication_lsn must be non-zero");
    let mut post_recovery_state = MapDeltaApplyState::recovered(descriptor, recovered_lsn);

    assert_eq!(
        post_recovery_state.applied_up_to_lsn,
        Some(NonZeroU64::new(50).unwrap()),
        "post-recovery apply state must be seeded from recovered publication lsn"
    );

    // Re-applying delta_50_to_60: delta_lsn=60 > applied_up_to=50 → Applied.
    let re_apply = post_recovery_state.try_apply(delta_50_to_60).unwrap();
    assert_eq!(
        re_apply,
        MapDeltaApplyOutcome::Applied {
            new_applied_up_to_lsn: NonZeroU64::new(60).unwrap(),
        },
        "post-crash re-apply of the same delta must succeed (cursor was lost)"
    );

    // Subsequent replay of the SAME delta is idempotent.
    let idempotent = post_recovery_state.try_apply(delta_50_to_60).unwrap();
    assert_eq!(
        idempotent,
        MapDeltaApplyOutcome::AlreadyApplied,
        "second replay after recovery must be idempotent ack"
    );
}
