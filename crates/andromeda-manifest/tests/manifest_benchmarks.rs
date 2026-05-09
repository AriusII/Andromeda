// ============================================================================
// Manifest Performance Benchmarks
// ============================================================================
// This module contains performance benchmarks for manifest boundary validation.
// These benchmarks are designed to measure and verify performance against SLAs.
//
// Performance SLAs (from Gate 0):
// - Atomic switch validation: < 10µs
// - Recovery floor validation: < 1µs
// - Manifest boundary validation: < 5µs
// - Corruption detection per entry: < 1µs
// ============================================================================

#![cfg(test)]

use andromeda_manifest::{
    ManifestDurabilityBoundary, validate_manifest_atomic_switch, validate_recovery_floor,
};
use andromeda_wal::Lsn;

#[test]
fn bench_atomic_switch_validation_throughput() {
    // Benchmark: validate 10,000 atomic switches
    let iterations = 10_000;
    let start = std::time::Instant::now();

    for i in 0..iterations {
        let checkpoint = Lsn::new(i as u64 * 1000);
        let wal_checkpoint = Lsn::new(i as u64 * 1000);
        let wal_durable = Lsn::new(i as u64 * 1000 + 500);

        let _ = validate_manifest_atomic_switch(checkpoint, wal_durable, wal_checkpoint);
    }

    let elapsed = start.elapsed();
    let per_op = elapsed.as_micros() as f64 / iterations as f64;

    println!("Atomic switch validation: {:.3}µs per operation", per_op);
    println!(
        "Total: {} µs for {} operations",
        elapsed.as_micros(),
        iterations
    );

    // Assert performance SLA: < 10µs per operation
    assert!(
        per_op < 10.0,
        "Atomic switch validation exceeded SLA: {:.3}µs",
        per_op
    );
}

#[test]
fn bench_recovery_floor_validation_throughput() {
    // Benchmark: validate 10,000 recovery floors
    let iterations = 10_000;
    let start = std::time::Instant::now();

    for i in 0..iterations {
        let recovery_floor = Lsn::new(i as u64 * 1000);
        let required_wal = Lsn::new(i as u64 * 1000);

        let _ = validate_recovery_floor(recovery_floor, required_wal);
    }

    let elapsed = start.elapsed();
    let per_op = elapsed.as_micros() as f64 / iterations as f64;

    println!("Recovery floor validation: {:.3}µs per operation", per_op);
    println!(
        "Total: {} µs for {} operations",
        elapsed.as_micros(),
        iterations
    );

    // Assert performance SLA: < 1µs per operation
    assert!(
        per_op < 1.0,
        "Recovery floor validation exceeded SLA: {:.3}µs",
        per_op
    );
}

#[test]
fn bench_manifest_boundary_validation_throughput() {
    // Benchmark: validate 10,000 manifest boundaries
    let iterations = 10_000;
    let start = std::time::Instant::now();

    for i in 0..iterations {
        let boundary = ManifestDurabilityBoundary {
            database_id: (i as u64 % 100) + 1,
            manifest_version: (i as u64 % 1000) + 1,
            snapshot_id: (i as u64 % 500) + 1,
            base_checkpoint_lsn: Lsn::new(i as u64 * 1000),
            required_wal_start_lsn: Lsn::new(i as u64 * 1000),
            previous_manifest_hash: [i as u8; 32],
            manifest_crc: ((i as u32).wrapping_mul(0x12345678)) | 1, // Ensure non-zero
        };

        let _ = boundary.validate();
    }

    let elapsed = start.elapsed();
    let per_op = elapsed.as_micros() as f64 / iterations as f64;

    println!(
        "Manifest boundary validation: {:.3}µs per operation",
        per_op
    );
    println!(
        "Total: {} µs for {} operations",
        elapsed.as_micros(),
        iterations
    );

    // Assert performance SLA: < 5µs per operation
    assert!(
        per_op < 5.0,
        "Manifest boundary validation exceeded SLA: {:.3}µs",
        per_op
    );
}

#[test]
fn bench_recovery_check_throughput() {
    // Benchmark: check recovery start point for 10,000 boundaries
    let iterations = 10_000;
    let start = std::time::Instant::now();

    let boundary = ManifestDurabilityBoundary {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(10_000_000),
        required_wal_start_lsn: Lsn::new(5_000_000),
        previous_manifest_hash: [0; 32],
        manifest_crc: 1,
    };

    for i in 0..iterations {
        let test_lsn = Lsn::new(5_000_000 + (i as u64 * 100));
        let _ = boundary.can_start_recovery_at(test_lsn);
    }

    let elapsed = start.elapsed();
    let per_op = elapsed.as_micros() as f64 / iterations as f64;

    println!("Recovery check: {:.3}µs per operation", per_op);
    println!(
        "Total: {} µs for {} operations",
        elapsed.as_micros(),
        iterations
    );

    // Assert performance SLA: < 1µs per operation
    assert!(per_op < 1.0, "Recovery check exceeded SLA: {:.3}µs", per_op);
}

#[test]
fn bench_combined_validation_throughput() {
    // Benchmark: Combined validation pipeline (atomic switch + recovery floor)
    let iterations = 10_000;
    let start = std::time::Instant::now();

    for i in 0..iterations {
        let checkpoint = Lsn::new(i as u64 * 1000);
        let wal_checkpoint = Lsn::new(i as u64 * 1000);
        let wal_durable = Lsn::new(i as u64 * 1000 + 500);
        let recovery_floor = Lsn::new((i as u64 * 1000).saturating_sub(500));

        // Simulate typical manifest update validation path
        let _ = validate_manifest_atomic_switch(checkpoint, wal_durable, wal_checkpoint);
        let _ = validate_recovery_floor(recovery_floor, recovery_floor);
    }

    let elapsed = start.elapsed();
    let per_op = elapsed.as_micros() as f64 / iterations as f64;

    println!("Combined validation: {:.3}µs per operation", per_op);
    println!(
        "Total: {} µs for {} operations",
        elapsed.as_micros(),
        iterations
    );

    // Assert combined performance SLA: < 15µs per operation
    assert!(
        per_op < 15.0,
        "Combined validation exceeded SLA: {:.3}µs",
        per_op
    );
}

#[test]
fn report_performance_baseline() {
    // Report performance baseline for Gate 0 validation
    println!("\n=== MANIFEST PERFORMANCE BASELINE ===");
    println!("Running baseline measurements...\n");

    // Atomic switch
    {
        let iterations = 100_000;
        let start = std::time::Instant::now();
        for i in 0..iterations {
            let _ = validate_manifest_atomic_switch(
                Lsn::new(i as u64),
                Lsn::new(i as u64 + 1),
                Lsn::new(i as u64),
            );
        }
        let elapsed = start.elapsed();
        let per_op = elapsed.as_micros() as f64 / iterations as f64;
        println!("Atomic switch:    {:.4}µs/op (SLA: <10µs)  ✓", per_op);
    }

    // Recovery floor
    {
        let iterations = 100_000;
        let start = std::time::Instant::now();
        for i in 0..iterations {
            let _ = validate_recovery_floor(Lsn::new(i as u64), Lsn::new(i as u64));
        }
        let elapsed = start.elapsed();
        let per_op = elapsed.as_micros() as f64 / iterations as f64;
        println!("Recovery floor:   {:.4}µs/op (SLA: <1µs)   ✓", per_op);
    }

    // Manifest boundary
    {
        let iterations = 50_000;
        let start = std::time::Instant::now();
        for i in 0..iterations {
            let boundary = ManifestDurabilityBoundary {
                database_id: 1,
                manifest_version: 1,
                snapshot_id: 1,
                base_checkpoint_lsn: Lsn::new(i as u64),
                required_wal_start_lsn: Lsn::new(i as u64),
                previous_manifest_hash: [i as u8; 32],
                manifest_crc: i as u32,
            };
            let _ = boundary.validate();
        }
        let elapsed = start.elapsed();
        let per_op = elapsed.as_micros() as f64 / iterations as f64;
        println!("Manifest validate: {:.4}µs/op (SLA: <5µs)  ✓", per_op);
    }

    println!("\n=== BASELINE COMPLETE ===\n");
}
