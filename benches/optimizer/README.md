# Optimizer Benchmark Suite

Benchmarks for query optimization and plan caching performance.

## Scenarios

- Plan compilation time
- Plan cache hit ratio
- Cardinality estimation accuracy
- Join order selection
- Predicate pushdown overhead

## Important Constraint

Benchmarks measure optimizer performance but are NOT the source of truth for correctness.

Each optimization must be validated through:
- Correctness tests (result accuracy)
- Cardinality validation
- Visible result consistency

## Hardware Profiles

Specify when adding benchmarks:
- CPU model
- Memory capacity
- Workload schema size

## Non-Goals

Benchmarks must not:
- Override correctness tests
- Justify skipping cardinality validation
- Replace row result verification
