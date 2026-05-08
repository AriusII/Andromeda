# Analytics Benchmark Suite

Benchmarks for analytics-specific operations (GPU-friendly, off-critical-path).

## Scenarios

- Columnar scan throughput
- SIMD vector operations (with scalar fallback)
- Aggregation performance
- Statistics histogram generation
- Analytics query execution time

## GPU Policy

GPU acceleration is analytics-only and must NOT participate in:
- Commit paths
- WAL logging
- Recovery/rollback
- Transaction visibility
- Security/audit decisions

All GPU results must have scalar (CPU) fallback implementations.

## Hardware Profiles

Specify when adding benchmarks:
- CPU model and SIMD capability (AVX2, AVX-512, NEON)
- GPU model (if used)
- Memory capacity

## Non-Goals

Analytics benchmarks must not:
- Override correctness for visible results
- Justify GPU use in critical paths
- Replace recovery validation
