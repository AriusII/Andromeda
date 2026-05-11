//! P09 / A2 gap-closure tests — descriptor snapshot isolation under concurrent publication switch.
//!
//! Traceability: closes P09 audit A2 gaps G1 (Major), G2 (Moderate), G3 (Moderate).
//!
//! ## What is being proven
//!
//! 1. **G1** — A `StatsObjectDescriptor` snapshot acquired at `StatsVersion(N)` is a `Copy`
//!    value that cannot be mutated by any concurrent `StatsPublicationSwitch` operation.
//!    "Rejected stats candidate must NEVER mutate active plans" is enforced at the value
//!    boundary — the snapshot carries the old version and the old digest immutably.
//!
//! 2. **G2** — The lifecycle "descriptor acquired → publication switches → descriptor stale →
//!    optimizer falls back" is exercised under deterministic concurrent access.
//!    `evaluate_statistics_for_optimizer` must not panic and must return
//!    `StaleRejected` (strict policy) or `AcceptedStaleByPolicy` (lenient policy).
//!
//! 3. **G3** — Multiple concurrent acquirers that snapshot the active publication after the
//!    same switch all see a consistent `StatsVersion`.
//!
//! ## Synchronization discipline
//!
//! All inter-thread ordering uses `std::sync::Barrier` or `std::sync::Mutex`.
//! No `std::thread::sleep` is used anywhere in this file.

use std::sync::{Arc, Barrier, Mutex};

use andromeda_decision_trace::DecisionTraceId;
use andromeda_observability::TraceId;
use andromeda_procedure_contract::{PolicyVersion, StatsVersion};
use andromeda_statistics::{
    HistogramBucket, HistogramPlaceholder, SkewMarker, StatisticsUsePolicy, StatisticsUseReason,
    StatsColumnTarget, StatsObjectDescriptor, StatsPublication, StatsPublicationBuilder,
    StatsPublicationDecisionEvidence, StatsPublicationState, StatsPublicationSwitch,
    StatsSetDigest, evaluate_statistics_for_optimizer,
};
use andromeda_types::{CatalogObjectId, CatalogVersion};

// ─── Fixtures ────────────────────────────────────────────────────────────────

/// Build a deterministic, valid `StatsPublication` at the given `stats_version`.
fn make_publication(stats_version: u64) -> StatsPublication {
    let target = StatsColumnTarget::new(CatalogObjectId::new(42), 1);
    let histogram = HistogramPlaceholder::new(
        vec![
            HistogramBucket {
                lower_inclusive: 0,
                upper_inclusive: 9,
                row_estimate: 50,
                distinct_estimate: 10,
            },
            HistogramBucket {
                lower_inclusive: 10,
                upper_inclusive: 19,
                row_estimate: 40,
                distinct_estimate: 10,
            },
        ],
        SkewMarker::LowSkew,
    )
    .expect("test histogram must be valid");

    StatsPublicationBuilder::new(CatalogVersion::new(2), StatsVersion::new(stats_version))
        .expect("test publication builder versions must be non-zero")
        .push(target, histogram)
        .expect("test histogram entry must be pushable")
        .finish()
}

/// Extract a `StatsObjectDescriptor` snapshot from the switch's current active publication.
///
/// This mirrors the caller responsibility: the consumer takes a *copy* of the
/// version + digest at a point in time, and the copy is thereafter independent.
fn descriptor_from_active(switch: &StatsPublicationSwitch) -> StatsObjectDescriptor {
    let active = switch
        .active()
        .expect("switch must have an active publication");
    let digest_bytes = active.digest().as_bytes();
    // StatsSetDigest and StatsPublicationDigest are distinct newtypes over [u8; 32].
    // The conversion is explicit and lossless.
    let stats_digest =
        StatsSetDigest::new(digest_bytes).expect("publication digest must never be all-zero");
    StatsObjectDescriptor::new(
        active.catalog_version(),
        active.version(),
        stats_digest,
        StatsPublicationState::Published,
    )
    .expect("descriptor from active publication must be valid")
}

/// Full three-step publish cycle: stage → validate → publish_validated_candidate.
///
/// `trace_base` must leave ≥ 3 non-zero trace IDs available: `trace_base`,
/// `trace_base + 1`, `trace_base + 2`, and `trace_base + 100` for evidence.
fn do_publish(
    switch: &mut StatsPublicationSwitch,
    publication: StatsPublication,
    trace_base: u128,
) {
    switch
        .stage_candidate(
            publication,
            TraceId::new(trace_base),
            "test: candidate staged for publication",
        )
        .expect("stage_candidate must succeed in test");

    let validation = switch
        .validate_candidate(
            TraceId::new(trace_base + 1),
            "test: candidate validated before publication switch",
        )
        .expect("validate_candidate must succeed in test");
    assert!(
        validation.accepted,
        "test candidate validation must be accepted"
    );

    let evidence =
        StatsPublicationDecisionEvidence::canonical_validation(TraceId::new(trace_base + 100))
            .expect("canonical validation evidence must be constructible");

    switch
        .publish_validated_candidate(
            TraceId::new(trace_base + 2),
            evidence,
            "test: validated candidate published as active StatsVersion",
        )
        .expect("publish_validated_candidate must succeed in test");
}

/// Build a non-zero `PolicyVersion` from a single repeated byte.
fn policy_version(byte: u8) -> PolicyVersion {
    PolicyVersion::new([byte; PolicyVersion::LEN])
}

// ─── Main traceability test ───────────────────────────────────────────────────

/// **Traceability: P09-A2 / G1 + G2 + G3**
///
/// Proves the three clustered gaps in a single deterministic integration test:
///
/// - `descriptor` — snapshot is a `Copy` value isolated from switch mutations (G1).
/// - `snapshot` — lifecycle "acquired → switch → stale → fallback" is covered (G2).
/// - `concurrent_publication_switch` — multiple acquirers see consistent versions (G3).
/// - `optimizer_falls_back` — `evaluate_statistics_for_optimizer` returns `StaleRejected`
///   or `AcceptedStaleByPolicy` without panicking (G2).
#[test]
fn descriptor_snapshot_survives_concurrent_publication_switch_and_optimizer_falls_back() {
    // ── Step 1: Publish N=7 on the main thread ───────────────────────────────
    //
    // `StatsPublicationSwitch` is not `Sync`; shared access uses `Arc<Mutex<>>`.
    let shared_switch = Arc::new(Mutex::new(StatsPublicationSwitch::new()));
    {
        let mut sw = shared_switch.lock().unwrap();
        do_publish(&mut sw, make_publication(7), 1_000);
        assert_eq!(
            sw.active().unwrap().version(),
            StatsVersion::new(7),
            "active publication must be StatsVersion(7) after initial publish"
        );
    }

    // ── Step 2: Acquire a descriptor snapshot at N=7 ─────────────────────────
    //
    // `StatsObjectDescriptor` is `Copy` — taking it creates an independent value.
    // No reference into the switch is held after this block.
    let descriptor_at_n7: StatsObjectDescriptor = {
        let sw = shared_switch.lock().unwrap();
        descriptor_from_active(&sw)
    };
    assert_eq!(
        descriptor_at_n7.stats_version(),
        StatsVersion::new(7),
        "snapshot must carry StatsVersion(7)"
    );
    assert!(
        !descriptor_at_n7.is_stale(),
        "freshly acquired snapshot must not be pre-marked stale"
    );
    assert!(
        descriptor_at_n7.is_published(),
        "snapshot state must be Published"
    );

    // ── Step 3: Concurrent publication switch N=7 → N=8 → N=9 ───────────────
    //
    // Barrier(2): main thread and publisher thread both call `.wait()`.
    // This guarantees ordering without any sleep.
    let barrier_before = Arc::new(Barrier::new(2));
    let barrier_after = Arc::new(Barrier::new(2));

    let switch_for_publisher = Arc::clone(&shared_switch);
    let b_before = Arc::clone(&barrier_before);
    let b_after = Arc::clone(&barrier_after);

    let publisher = std::thread::spawn(move || {
        // Signal: publisher is ready, both threads proceed simultaneously.
        b_before.wait();

        {
            let mut sw = switch_for_publisher.lock().unwrap();
            // Publish N+1 = 8
            do_publish(&mut sw, make_publication(8), 2_000);
            // Publish N+2 = 9 — second switch on same thread.
            do_publish(&mut sw, make_publication(9), 3_000);
        }

        // Signal: publication N+2 complete.
        b_after.wait();
    });

    // Release the publisher.
    barrier_before.wait();
    // Wait until both N+1 and N+2 are active.
    barrier_after.wait();

    // ── Step 4: Verify switch advanced to N=9 ────────────────────────────────
    {
        let sw = shared_switch.lock().unwrap();
        assert_eq!(
            sw.active().unwrap().version(),
            StatsVersion::new(9),
            "switch must have advanced to StatsVersion(9) after concurrent publication"
        );
    }

    // ── G1 assertion: original snapshot is completely unmodified ─────────────
    //
    // `descriptor_at_n7` is a Copy value on the stack.  No publication switch
    // can mutate it — this is the boundary invariant.
    assert_eq!(
        descriptor_at_n7.stats_version(),
        StatsVersion::new(7),
        "G1: descriptor snapshot must NOT be mutated by any concurrent publication switch"
    );
    assert!(
        !descriptor_at_n7.is_stale(),
        "G1: stale bit must not be auto-set by the switch — marking is caller responsibility"
    );
    assert!(
        descriptor_at_n7.is_published(),
        "G1: snapshot publication state must remain Published; no side-channel mutation"
    );

    // ── G2 lifecycle: mark stale, then call evaluate_statistics_for_optimizer ─
    //
    // After observing that the switch has advanced past N=7, the caller is
    // responsible for calling `.mark_stale()`.  This is the contract.
    let stale_descriptor = descriptor_at_n7.mark_stale();
    assert!(
        stale_descriptor.is_stale(),
        "mark_stale must produce a stale descriptor"
    );
    assert_eq!(
        stale_descriptor.stats_version(),
        StatsVersion::new(7),
        "mark_stale must preserve the original StatsVersion"
    );
    // The original snapshot is unaffected (Copy semantics).
    assert!(
        !descriptor_at_n7.is_stale(),
        "mark_stale must not modify the original Copy value"
    );

    // Sub-case A: strict policy → StaleRejected (Fallback outcome)
    let strict_policy = StatisticsUsePolicy::enabled(policy_version(0xAA));
    let decision_strict = evaluate_statistics_for_optimizer(
        Some(stale_descriptor),
        strict_policy,
        DecisionTraceId::new(9_001).unwrap(),
    )
    .expect("evaluate_statistics_for_optimizer must not error with valid inputs");

    assert_eq!(
        decision_strict.reason(),
        StatisticsUseReason::StaleRejected,
        "G2: strict policy must return StaleRejected for a stale descriptor"
    );
    assert!(
        !decision_strict.accepted(),
        "G2: stale descriptor must not be accepted under strict policy"
    );
    // Critical: the decision carries back exactly the passed descriptor.
    // No mutation occurred — the plan can verify its own pinned version.
    assert_eq!(
        decision_strict.descriptor(),
        Some(stale_descriptor),
        "G2: decision descriptor must be the exact stale snapshot — no mutation permitted"
    );
    // The trace must be observable (not disabled, not redacted).
    assert!(
        decision_strict.trace().is_observable(),
        "G2: StaleRejected decision trace must be observable"
    );

    // Sub-case B: lenient policy (allow_stale=true) → AcceptedStaleByPolicy
    let lenient_policy = StatisticsUsePolicy::enabled(policy_version(0xBB)).with_stale_allowed();
    let decision_lenient = evaluate_statistics_for_optimizer(
        Some(stale_descriptor),
        lenient_policy,
        DecisionTraceId::new(9_002).unwrap(),
    )
    .expect("evaluate_statistics_for_optimizer must not error with lenient policy");

    assert_eq!(
        decision_lenient.reason(),
        StatisticsUseReason::AcceptedStaleByPolicy,
        "G2: lenient policy must return AcceptedStaleByPolicy for a stale descriptor"
    );
    assert!(
        decision_lenient.accepted(),
        "G2: stale descriptor must be accepted under lenient (allow_stale) policy"
    );
    assert_eq!(
        decision_lenient.descriptor(),
        Some(stale_descriptor),
        "G2: lenient decision descriptor must still be the exact stale snapshot"
    );

    // Sub-case C: disabled policy → DisabledByPolicy (even with a valid descriptor)
    let disabled_policy = StatisticsUsePolicy::disabled(policy_version(0xCC));
    let decision_disabled = evaluate_statistics_for_optimizer(
        Some(stale_descriptor),
        disabled_policy,
        DecisionTraceId::new(9_003).unwrap(),
    )
    .expect("evaluate_statistics_for_optimizer must not error with disabled policy");

    assert_eq!(
        decision_disabled.reason(),
        StatisticsUseReason::DisabledByPolicy,
        "G2: disabled policy must gate out even a stale descriptor"
    );
    assert!(
        !decision_disabled.accepted(),
        "G2: disabled policy must not accept any descriptor"
    );

    // ── G1 final: switch active is still N=9, unmodified by any optimizer call ─
    {
        let sw = shared_switch.lock().unwrap();
        assert_eq!(
            sw.active().unwrap().version(),
            StatsVersion::new(9),
            "G1: switch active version must not be mutated by evaluate_statistics_for_optimizer"
        );
        // History must include all published transitions (3 publishes = 9 trace entries: 3×3).
        assert!(
            sw.history().len() >= 9,
            "G1: switch history must accumulate all publication decision traces"
        );
    }

    // ── G3: Multiple concurrent acquirers see a consistent StatsVersion ───────
    //
    // With N=9 active, spawn two reader threads behind a Barrier(3).
    // All threads are released simultaneously.  Because `Mutex` serializes
    // access, each reader sees a coherent state.  Both must observe N=9.
    let barrier_readers = Arc::new(Barrier::new(3)); // main + reader1 + reader2

    let switch_r1 = Arc::clone(&shared_switch);
    let switch_r2 = Arc::clone(&shared_switch);
    let b_r1 = Arc::clone(&barrier_readers);
    let b_r2 = Arc::clone(&barrier_readers);

    let reader1 = std::thread::spawn(move || {
        b_r1.wait();
        let sw = switch_r1.lock().unwrap();
        descriptor_from_active(&sw)
    });

    let reader2 = std::thread::spawn(move || {
        b_r2.wait();
        let sw = switch_r2.lock().unwrap();
        descriptor_from_active(&sw)
    });

    // Release all three simultaneously.
    barrier_readers.wait();

    let desc_r1 = reader1.join().expect("reader1 must not panic");
    let desc_r2 = reader2.join().expect("reader2 must not panic");

    assert_eq!(
        desc_r1.stats_version(),
        StatsVersion::new(9),
        "G3: reader1 must observe current StatsVersion(9)"
    );
    assert_eq!(
        desc_r2.stats_version(),
        StatsVersion::new(9),
        "G3: reader2 must observe current StatsVersion(9)"
    );
    assert_eq!(
        desc_r1.stats_version(),
        desc_r2.stats_version(),
        "G3: all concurrent acquirers must see a consistent StatsVersion"
    );
    assert_eq!(
        desc_r1.digest(),
        desc_r2.digest(),
        "G3: all concurrent acquirers must see a consistent publication digest"
    );
    assert!(
        !desc_r1.is_stale(),
        "G3: freshly acquired snapshot from current active must not be stale"
    );
    assert!(
        !desc_r2.is_stale(),
        "G3: freshly acquired snapshot from current active must not be stale"
    );

    // ── Epilogue: publisher thread must have completed without panic ──────────
    publisher
        .join()
        .expect("publisher thread must not panic — no concurrency hazard detected");
}
