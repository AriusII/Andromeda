# Testing

This directory contains testing strategy, guidelines, and infrastructure documentation.

## Purpose

Testing documentation covers:
- Overall test strategy and philosophy
- Test organization and naming conventions
- Fuzzing infrastructure and targets
- Crash injection and recovery testing
- Property-based testing
- Performance benchmarking

## Contents

### Test Strategy

- **test-strategy.md** - Testing pyramid, coverage goals, risk-based testing approach
- **test-naming-conventions.md** - Test naming patterns by subsystem and type
- **integration-test-design.md** - End-to-end test scenarios and fixtures

### Crash and Recovery Testing

- **crash-injection-matrix.md** - Crash points by subsystem, failure scenarios
- **recovery-test-design.md** - Recovery validation, idempotency proofs
- **forensic-startup-testing.md** - ForensicStart procedures, corruption detection

### Fuzzing

- **fuzzing-strategy.md** - Fuzzing targets, corpus management, duration policies
- **fuzzing-targets.md** - Per-crate fuzzing targets and entry points
- **libfuzzer-setup.md** - Running local fuzzing, CI fuzzing integration

### Property-Based Testing

- **property-testing-approach.md** - proptest usage, property definitions
- **invariant-properties.md** - Formally verified properties by subsystem

### Performance and Benchmarking

- **benchmark-workload-design.md** - Benchmark design principles
- **benchmark-methodology.md** - Hardware profiles, baseline establishment, regression detection
- **profiling-guidelines.md** - Using perf, flamegraph, and frame pointers

### Concurrency and Synchronization

- **loom-testing.md** - Loom model checking for concurrency bugs
- **miri-unsafety-testing.md** - Miri for undefined behavior detection

### SRPL Testing

- **srpl-type-system-testing.md** - Type checker validation, error cases
- **srpl-semantics-validation.md** - Execution semantics, cardinality correctness

## Test Organization

Tests are organized by:
- Subsystem (tests/wal/, tests/storage/, tests/catalog/, etc.)
- Type (integration, recovery, fuzz, security)
- Component (per-crate unit tests)

## Running Tests

See .config/nextest.toml for test profiles:
```bash
cargo nextest run --profile default      # Fast dev tests
cargo nextest run --profile ci           # Full CI run
cargo nextest run --profile recovery     # Crash/recovery only
cargo nextest run --profile wal          # WAL subsystem
```

## Cross-References

- Architecture (docs/architecture/) for test targets
- Specifications (docs/specifications/) for what to verify
