# andromeda-plan-cache

## Purpose

`andromeda-plan-cache` owns plan cache identity, version binding, bounded cache policy, invalidation evidence, and plan reuse DecisionTrace contracts.

The crate exposes the v0 plan-cache key, bounded plan class taxonomy, shape fingerprinting, advisory evidence summaries, bounded in-memory cache gate, and disableable reuse policy. It stores only opaque plan identity evidence, not executable plans.

## Scope

This crate is expected to own:

- Strict `PlanCacheKey` identity across procedure, contract hash, catalog version, stats version, policy version, and plan class.
- Cache entry compatibility and invalidation rules.
- Bounded in-memory cache policy and disablement controls.
- DecisionTrace evidence for hits, misses, invalidations, stale entries, and rejected advisory evidence.
- ScenarioEvidence admission status when benchmark-derived signals are considered.

## Non-goals

- Do not reuse plans across incompatible catalog, contract, statistics, or policy versions.
- Do not treat cache hits as durable truth.
- Do not let benchmark output or ScenarioEvidence force a plan cache entry.
- Do not place cache maintenance, benchmark evidence, analytics, or GPU work in C5 commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not use native Rust layout as a persistent or network format.

## Prerequisites

- Keep catalog publication and executable plan ownership outside this crate unless a registered extraction work order moves those integrations.
- Require complete version bindings before admitting any plan entry.
- Keep all advisory evidence expirable, rejectable, and traceable.

## Procedure

1. Define identity and compatibility before moving cache storage.
2. Reject incomplete or stale `PlanCacheKey` values.
3. Keep cache admission bounded and disableable.
4. Record DecisionTrace evidence for cache hit, miss, invalidation, and fallback paths.
5. Preserve current catalog compatibility imports until callers migrate.

## Validation

Future behavior changes should use:

```powershell
cargo test -p andromeda-plan-cache
cargo test -p andromeda-catalog --test catalog_server_runtime_contract -- --nocapture
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

Use this crate for plan-cache identity, bounded reuse policy, and traceable cache-gate validation.

## Troubleshooting

- If a cache entry lacks a stats or policy version, reject it.
- If benchmark evidence changes cache admission directly, route it through advisory validation and DecisionTrace.
- If cache behavior becomes required for correctness, move the correctness rule to the owning catalog or execution path.

## References

- [Workspace crate rules](../README.md)
- [Current catalog owner](../andromeda-catalog/README.md)
- [Current observability owner](../andromeda-observe/README.md)
