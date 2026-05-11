//! P10 exit-criterion test: "Map maintenance ne starve jamais WAL flush"
//!
//! This file contains two tests:
//!
//! 1. `wal_fsync_latency_is_not_starved_by_map_refresh_workload` — latency
//!    isolation proof: WAL flush latency stays within K×baseline under a
//!    sustained Map refresh CPU load.
//!
//! 2. `wal_queue_depth_metrics_drive_map_refresh_admission_gate` — feedback
//!    loop proof: reading `InMemoryWal::queue_depth_metrics()` and injecting
//!    the value into `MapRefreshAdmissionRequest` correctly gates Map refresh
//!    when the WAL backlog grows, and resumes admission after WAL flushes.
//!
//! # Simulated Map Refresh Stand-in
//!
//! Map refresh executor wiring does not yet exist in this codebase.
//! `andromeda-maps` is a policy-only crate (admission + consistency types;
//! no executor runtime). The stand-in is `LOAD_THREADS` background threads,
//! each running a tight loop calling `MapConsistencyPolicy::admit()`.  This
//! exercises the same CPU and memory-bus resources (branch prediction,
//! allocator pressure, stack-frame creation) that a real Map refresh
//! controller would consume.  The WAL writer (`InMemoryWal`) uses heap +
//! CPU-only compute; both compete on the same CPU scheduler — the intended
//! contention domain.
//!
//! When the Map refresh executor is wired in a later phase, the stand-in
//! loop in this test should be replaced with a real `MapRefreshTask` spawn.

#![allow(missing_docs)] // integration-test helpers do not require doc-comments

use std::hint::black_box;
use std::num::{NonZeroU16, NonZeroU64};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use andromeda_analytics::{
    AnalyticsAccelerationPolicy, AnalyticsExecutionBounds, AnalyticsJobDescriptor,
    AnalyticsWorkloadKind,
};
use andromeda_columnar::{
    ColumnChunkDescriptor, ColumnChunkPruningMetadata, ColumnarAccelerationPolicy,
    ColumnarConsumer, ColumnarLayoutDescriptor, ColumnarSegmentDescriptor, ColumnarSnapshotBinding,
    ColumnarVersionBinding,
};
use andromeda_maps::{
    MapConsistencyPolicy, MapDescriptor, MapDescriptorError, MapGrain, MapId,
    MapRefreshAdmissionRequest, MapRefreshMode, MapRefreshPlan, MapStalenessPolicy,
};
use andromeda_wal::{InMemoryWal, WalQueueSeparationEvidence, WalRecordKind};

// ---------------------------------------------------------------------------
// Latency-isolation constants
// ---------------------------------------------------------------------------

/// Ratio multiplier: WITH-load latency must not exceed K × baseline latency.
///
/// **Rationale for K = 5.0:**
///
/// * Structural lock coupling (a shared `Mutex`/`RwLock` between the WAL
///   writer and the Map refresh admission path) would block the WAL thread
///   for full scheduler timeslices (≈ 10–50 ms each), producing K >> 50.
///   K = 5.0 is orders of magnitude below this failure threshold.
///
/// * A 5× budget comfortably accommodates:
///   - Single-core CI environments where `LOAD_THREADS` background threads
///     preempt the WAL thread (scheduling jitter can cause 2–3× slowdown
///     without any lock coupling).
///   - OS memory-allocator lock contention amortised over
///     `MEASUREMENT_ROUNDS` rounds.
///
/// * Validated across 5 sequential runs of the test to confirm
///   non-flakiness (see W1_REPORT.md).
const K: f64 = 5.0;

/// Rounds per measurement phase.  200 rounds amortises per-round OS
/// scheduling noise better than smaller sample sizes.
const MEASUREMENT_ROUNDS: u32 = 200;

/// Warm-up rounds discarded before each measurement phase to prime the
/// heap allocator and instruction caches.
const WARMUP_ROUNDS: u32 = 20;

/// Number of background Map refresh admission threads.
///
/// Two threads provide meaningful CPU pressure while remaining safe on
/// single-core CI environments (three threads would leave the WAL thread with
/// ≈ 25 % of one core; two threads leave ≈ 33 %).
const LOAD_THREADS: usize = 2;

/// Minimum baseline duration used for the ratio comparison.  Prevents
/// micro-second baselines from making the K× bound unrealistically tight
/// when the OS clock resolution or scheduler noise dominates.
const BASELINE_FLOOR: Duration = Duration::from_micros(500);

// ---------------------------------------------------------------------------
// Shared test fixtures
// ---------------------------------------------------------------------------

fn make_descriptor() -> MapDescriptor {
    MapDescriptor::new(
        MapId::new(2001).unwrap(),
        MapGrain::Relation,
        MapRefreshMode::Deferred,
        MapStalenessPolicy::AdvisorySnapshot,
    )
}

fn make_cpu_first_plan(descriptor: MapDescriptor) -> MapRefreshPlan {
    let job = AnalyticsJobDescriptor::new(
        AnalyticsWorkloadKind::MapRefresh,
        AnalyticsExecutionBounds::new(
            NonZeroU64::new(50_000).unwrap(),
            NonZeroU64::new(32 * 1024 * 1024).unwrap(),
            true, // cancellation_required = true (advisory job)
        ),
        AnalyticsAccelerationPolicy::CpuOnly,
    );

    let layout = ColumnarLayoutDescriptor::new(
        ColumnarConsumer::Maps,
        NonZeroU16::new(4).unwrap(),
        ColumnarVersionBinding::new(
            NonZeroU64::new(1).unwrap(),
            NonZeroU64::new(2).unwrap(),
            NonZeroU64::new(3).unwrap(),
            None,
        ),
        ColumnarAccelerationPolicy::CpuOnly,
    );

    let segment = ColumnarSegmentDescriptor::new(
        layout,
        ColumnarSnapshotBinding::new(NonZeroU64::new(50).unwrap()),
        vec![ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(1024).unwrap(),
            ColumnChunkPruningMetadata::new(true, true, false),
        )],
    );

    MapRefreshPlan::new(descriptor, job, Some(segment)).unwrap()
}

/// Policy: max_refresh_cost = 100, max_wal_queue_depth = 8.
fn make_policy(descriptor: MapDescriptor) -> MapConsistencyPolicy {
    MapConsistencyPolicy::new(
        descriptor,
        NonZeroU64::new(100).unwrap(),
        NonZeroU64::new(8).unwrap(),
    )
}

// ---------------------------------------------------------------------------
// WAL measurement helper
// ---------------------------------------------------------------------------

/// Run `rounds` of "append 3 `PageFormat` WAL records + flush_all", and
/// return the **total** elapsed wall-clock duration.
///
/// `PageFormat` is used because it does not require a `TransactionId`
/// (avoiding an `andromeda-types` dev-dependency while still exercising
/// the full WAL append → checksum → flush path).
///
/// A fresh `InMemoryWal` is created each round so that the pending-records
/// scan inside `flush_all` always operates on a small, bounded set (3
/// records), preventing O(n²) growth across the measurement phase.
fn measure_wal_rounds(rounds: u32) -> Duration {
    let t0 = Instant::now();
    for i in 0..rounds {
        let mut wal = InMemoryWal::new();
        // Three records simulate a minimal logical-WAL transaction: begin,
        // mutation, end — without needing a TransactionId.
        wal.append_payload(WalRecordKind::PageFormat, None, vec![0u8; 16])
            .expect("append record 1");
        wal.append_payload(WalRecordKind::PageFormat, None, vec![i as u8; 32])
            .expect("append record 2");
        wal.append_payload(WalRecordKind::PageFormat, None, vec![0u8; 16])
            .expect("append record 3");

        // flush_all() advances durable_lsn to last_lsn — the "fsync"
        // equivalent for InMemoryWal.
        let durable = black_box(wal.flush_all().expect("flush_all"));
        assert!(durable.get() >= 3, "expected at least 3 records flushed");
    }
    t0.elapsed()
}

// ---------------------------------------------------------------------------
// PRIMARY test: latency isolation under Map refresh CPU load
// ---------------------------------------------------------------------------

/// P10 exit criterion: "Map maintenance ne starve jamais WAL flush"
///
/// # What is proven
///
/// `InMemoryWal::append_payload` + `flush_all` latency (total for
/// `MEASUREMENT_ROUNDS` rounds) does not increase by more than K = 5.0×
/// when `LOAD_THREADS` background threads are concurrently executing a
/// tight `MapConsistencyPolicy::admit()` admission loop.
///
/// # Simulated stand-in (documented)
///
/// The Map refresh executor is not yet wired.  The stand-in is a tight loop
/// of `MapConsistencyPolicy::admit()` calls on background threads.  This is
/// the CPU-dominant work that a real Map refresh controller performs during
/// admission, and it exercises the same heap allocator and CPU branch
/// predictor that the WAL writer uses.  There is no shared mutex between
/// the two paths; the test would produce K >> 50 if one were introduced.
///
/// # Determinism
///
/// * Both phases use identical loop structure and a fresh `InMemoryWal`
///   per round, so per-round overhead is O(1) in both phases.
/// * A warm-up pass before each measurement phase primes the allocator.
/// * The `BASELINE_FLOOR` prevents micro-second baseline durations from
///   making the K× bound pathologically tight relative to clock resolution.
#[test]
fn wal_fsync_latency_is_not_starved_by_map_refresh_workload() {
    let descriptor = make_descriptor();
    let plan = make_cpu_first_plan(descriptor);
    let policy = make_policy(descriptor);

    // ------------------------------------------------------------------
    // Phase 0a: warm-up before baseline (allocator / i-cache priming)
    // ------------------------------------------------------------------
    measure_wal_rounds(WARMUP_ROUNDS);

    // ------------------------------------------------------------------
    // Phase 1: baseline — measure WAL rounds with no concurrent load
    // ------------------------------------------------------------------
    let baseline_total = measure_wal_rounds(MEASUREMENT_ROUNDS);

    // ------------------------------------------------------------------
    // Phase 0b: warm-up before loaded phase
    // ------------------------------------------------------------------
    measure_wal_rounds(WARMUP_ROUNDS);

    // ------------------------------------------------------------------
    // Phase 2: load — spawn LOAD_THREADS background threads performing a
    // tight MapConsistencyPolicy::admit() loop, then measure WAL rounds
    // ------------------------------------------------------------------
    let stop = Arc::new(AtomicBool::new(false));
    let handles: Vec<_> = (0..LOAD_THREADS)
        .map(|thread_idx| {
            let stop_clone = Arc::clone(&stop);
            // Clone plan and copy policy so each thread owns its state.
            // Neither type shares mutable state with the WAL writer.
            let plan_clone = plan.clone();
            let policy_copy = policy;

            thread::spawn(move || {
                let mut counter: u64 = (thread_idx as u64).wrapping_mul(997);
                while !stop_clone.load(Ordering::Relaxed) {
                    // Vary parameters each iteration to prevent dead-code
                    // elimination and exercise different admit() branches.
                    counter = counter.wrapping_add(1);

                    // cost: 1..=90 (within max_refresh_cost=100)
                    let cost = 1 + (counter % 90);
                    // staleness: 0..=199 ms; AdvisorySnapshot always passes
                    let staleness = counter % 200;
                    // wal_queue_depth: 0..=9; policy max_wal_queue_depth=8,
                    // so depth ∈ [0,8] succeeds and depth ∈ [9] fails with
                    // WalPriorityExceeded — a mix of admit paths.
                    let wal_depth = counter % 10;

                    let request = MapRefreshAdmissionRequest::new(cost, staleness, wal_depth)
                        .with_table_wal_records(2)
                        .with_map_wal_records(1)
                        .with_decision_trace();

                    // catalog_version and stats_version must be non-zero.
                    let result = black_box(policy_copy.admit(
                        &plan_clone,
                        request,
                        counter + 1,
                        counter + 2,
                    ));

                    // Accept both Ok (admitted) and Err(WalPriorityExceeded)
                    // (blocked) — both paths exercise admit() code.
                    // Any unexpected error is a test bug.
                    match result {
                        Ok(_) | Err(MapDescriptorError::WalPriorityExceeded) => {},
                        Err(e) => {
                            panic!("unexpected admit() error in load thread {thread_idx}: {e:?}")
                        },
                    }
                }
            })
        })
        .collect();

    let loaded_total = measure_wal_rounds(MEASUREMENT_ROUNDS);

    // Signal load threads to stop and join them.
    stop.store(true, Ordering::Relaxed);
    for handle in handles {
        handle.join().expect("load thread panicked");
    }

    // ------------------------------------------------------------------
    // Assertion: WITH-load total ≤ K × max(baseline_total, BASELINE_FLOOR)
    // ------------------------------------------------------------------
    // The BASELINE_FLOOR prevents a pathologically fast baseline from
    // making K× unrealistically tight relative to OS clock resolution.
    let effective_baseline = baseline_total.max(BASELINE_FLOOR);

    let limit = effective_baseline.mul_f64(K);
    assert!(
        loaded_total <= limit,
        "WAL flush latency was starved by Map refresh workload: \
         under-load={loaded_total:?}, baseline={baseline_total:?}, \
         effective_baseline={effective_baseline:?}, \
         limit={limit:?} ({K}× effective_baseline). \
         If a shared Mutex/RwLock between WAL writer and Map refresh \
         admission were introduced, the ratio would exceed K >> 50."
    );
}

// ---------------------------------------------------------------------------
// SECONDARY test: WAL queue-depth feedback loop drives admission gate
// ---------------------------------------------------------------------------

/// Proves the WAL → Map refresh admission feedback loop:
///
/// 1. Append records to an `InMemoryWal` to build a backlog.
/// 2. Read `InMemoryWal::queue_depth_metrics()` to obtain live queue depth.
/// 3. Inject the depth into `MapRefreshAdmissionRequest`.
/// 4. Assert `WalPriorityExceeded` when depth > policy threshold.
/// 5. Flush WAL to reduce the backlog.
/// 6. Re-read metrics, re-submit admission — assert it now succeeds.
///
/// This proves C-1 from the A6 audit: the caller CAN read the live WAL
/// queue depth via `WalQueueDepthMetrics::queued_records` and that value
/// correctly gates Map refresh admission.  (The full type-safe bridge
/// requested in A6/C-1 remains a future mission; this test documents the
/// current integration contract.)
#[test]
fn wal_queue_depth_metrics_drive_map_refresh_admission_gate() {
    let descriptor = make_descriptor();
    let plan = make_cpu_first_plan(descriptor);
    // Policy: max_wal_queue_depth = 8.  Depth > 8 → WalPriorityExceeded.
    let policy = make_policy(descriptor);

    let mut wal = InMemoryWal::new();
    let separation = WalQueueSeparationEvidence::p0_durable();

    // Append 12 PageFormat records without flushing any.
    for _ in 0..12 {
        wal.append_payload(WalRecordKind::PageFormat, None, vec![0u8; 8])
            .expect("append record");
    }

    // ------------ Step A: WAL backlog = 12 > policy max (8) ----------
    let depth_a = wal
        .queue_depth_metrics(separation)
        .expect("queue_depth_metrics with 12 pending");

    assert_eq!(
        depth_a.queued_records, 12,
        "expected 12 pending WAL records"
    );
    assert!(depth_a.has_backlog());

    // Feed the live depth into the admission request.
    let request_a = MapRefreshAdmissionRequest::new(
        50,                     // refresh_cost within budget
        0,                      // staleness: CurrentOnly satisfied
        depth_a.queued_records, // ← live WAL queue depth injected here
    )
    .with_table_wal_records(2)
    .with_map_wal_records(1)
    .with_decision_trace();

    let err_a = policy
        .admit(&plan, request_a, 100, 200)
        .expect_err("admission must be rejected when WAL backlog > max_wal_queue_depth");

    assert_eq!(
        err_a,
        MapDescriptorError::WalPriorityExceeded,
        "expected WalPriorityExceeded when wal_queue_depth ({}) > max (8)",
        depth_a.queued_records
    );

    // ------------ Step B: flush 10 of 12 records → backlog = 2 ------
    // LSN 10 is the 10th record.
    use andromeda_wal::Lsn;
    wal.flush_through(Lsn::new(10))
        .expect("flush_through LSN 10");

    let depth_b = wal
        .queue_depth_metrics(separation)
        .expect("queue_depth_metrics after partial flush");

    assert_eq!(
        depth_b.queued_records, 2,
        "expected 2 pending WAL records after flushing 10 of 12"
    );

    // depth_b.queued_records (2) is well below max_wal_queue_depth (8).
    let request_b = MapRefreshAdmissionRequest::new(
        50,
        0,
        depth_b.queued_records, // ← live WAL queue depth after flush
    )
    .with_table_wal_records(2)
    .with_map_wal_records(1)
    .with_decision_trace();

    let decision_b = policy
        .admit(&plan, request_b, 100, 200)
        .expect("admission must succeed when WAL backlog is within policy threshold");

    assert!(
        decision_b.preserves_wal_priority(),
        "admitted decision must preserve WAL priority"
    );
    assert_eq!(
        decision_b.wal_records_reserved, 3,
        "deferred refresh should reserve table (2) + map (1) WAL records"
    );
}
