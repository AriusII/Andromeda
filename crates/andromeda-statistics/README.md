# andromeda-statistics

## Purpose

`andromeda-statistics` owns versioned statistics descriptors, histogram summaries, skew metadata, correlation evidence, and optimizer-use contracts.

The crate exposes statistics builders, publication digests, descriptors, stale-use policy, and DecisionTrace-backed optimizer-use decisions. Catalog remains responsible for active publication switching where catalog-local ScenarioEvidence is still required.

## Scope

This crate is expected to own:

- `StatsVersion` identity and compatibility rules.
- Catalog, contract, policy, and collection-shape bindings for published statistics.
- Histogram, NDV, correlation, skew, and sampling descriptors.
- Validation rules that let optimizers decide whether statistics are fresh enough to use.
- DecisionTrace inputs that explain which statistics were used, ignored, rejected, or considered stale.

## Non-goals

- Do not treat statistics as durable database truth.
- Do not make benchmark output, GPU output, RAM state, or temporary files authoritative for statistics publication.
- Do not choose execution plans directly from statistics without bounded optimizer logic and DecisionTrace.
- Do not put statistics refresh or GPU-assisted statistics work in C5 commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not serialize Rust native structs directly to disk or network.

## Prerequisites

- Keep catalog publication switching and catalog-local ScenarioEvidence integrations outside this crate until a registered extraction work order moves them.
- Bind every published statistics view to explicit catalog, contract, policy, and statistics versions.
- Require CPU fallbacks for any future acceleration path.
- Treat benchmark and ScenarioEvidence input as advisory only.

## Procedure

1. Define statistics identities before moving builders or publication records.
2. Preserve current catalog-facing compatibility paths during extraction.
3. Add stale-version rejection before allowing optimizer consumers.
4. Emit DecisionTrace evidence for every accepted or rejected statistics input.
5. Validate that benchmark output can inform investigation but cannot publish statistics by itself.

## Validation

Future behavior changes should use:

```powershell
cargo test -p andromeda-statistics
cargo test -p andromeda-catalog --test catalog_publication_subscription -- --nocapture
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

Run package checks and focused tests when changing these contracts.

## Troubleshooting

- If an optimizer result depends on unversioned statistics, reject the path and add a complete version binding.
- If a statistics refresh depends on GPU output, require a CPU fallback and prove the work is outside C5.
- If benchmark output appears to publish statistics directly, move it behind advisory evidence validation.

## References

- [Workspace crate rules](../README.md)
- [Current catalog owner](../andromeda-catalog/README.md)
- [Current benchmark owner](../andromeda-bench/README.md)
