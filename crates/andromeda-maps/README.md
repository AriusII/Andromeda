# andromeda-maps

## Purpose

`andromeda-maps` defines runtime-free Map descriptors and publication evidence primitives.

Use this crate when an Andromeda component needs to describe Map identity, grain, refresh mode, staleness policy, or the evidence required to publish, roll back, rebuild, or recover a Map projection.

## Scope

This crate owns:

- `MapId`, `MapGrain`, `MapRefreshMode`, `MapStalenessPolicy`, and `MapDescriptor`.
- Publication candidates and validated publication candidates.
- Publication evidence that binds Map projections to catalog, statistics, and WAL-related versions supplied by owning components.
- Switch, rollback, rebuild, and recovery evidence shapes for Map publication state transitions.
- Typed descriptor errors for invalid Map metadata and missing durable evidence.

The crate models the contract for Map projection evidence. It does not materialize Maps or make projections durable by itself.

## Non-goals

- Do not treat a Map projection as source truth.
- Do not own catalog storage, statistics collection, WAL durability, recovery replay, storage manifests, GPU execution, or refresh scheduling.
- Do not make a candidate visible without the owning durable publication path.
- Do not add application-facing ad hoc SQL, dynamic predicates, or shape-shifting return semantics.
- Do not claim release or runtime readiness from descriptor-level tests alone.

## Ownership

`andromeda-maps` owns runtime-free Map metadata and publication evidence types.

Catalog, statistics, WAL, storage, execution, and recovery owners remain responsible for creating durable evidence, enforcing publication order, rebuilding projections from truth sources, and validating crash/recovery behavior. This crate may reject missing evidence, but it does not persist or replay that evidence.

## Validation

For documentation-only changes, check that this README keeps the required headings and does not describe Maps as durable truth.

For source changes in this crate, prefer:

```powershell
cargo test -p andromeda-maps
```

Run targeted publication contract tests when changing evidence or state-transition semantics.

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/descriptor.rs`
- `src/publication.rs`
- `tests/map_publication_contract.rs`
- `../README.md`
- `../../AGENTS.md`
