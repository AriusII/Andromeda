# CatalogDiff v0 Specification

## Purpose

Define the roadmap contract for `CatalogDiff v0`, the bounded evidence model
that compares catalog object versions across two catalog states and classifies
the change as additive, breaking, deprecated, unchanged, or rejected.

`CatalogDiff v0` is catalog evidence. It is not an application-facing query
surface, not an ad hoc migration language, and not database truth by itself.
Database truth remains the latest valid snapshot plus durable WAL, and a diff
is accepted only when it can be tied back to durable catalog publication
evidence.

## Scope

This specification applies to catalog object version comparison for
DefinitionBatch dry-run, compatibility review, catalog publication, plan-cache
invalidation, audit evidence, and recovery explanation.

It covers:

- diff inputs and identity rules;
- object version diff records;
- additive, breaking, deprecated, unchanged, and rejected lifecycle classes;
- Procedure contract compatibility integration;
- dependency and plan-cache impact evidence;
- deterministic ordering and diagnostic requirements;
- current implementation evidence and pending implementation gaps.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL, generic command text, dynamic table
  names, dynamic predicates, or shape-shifting returns;
- bypass typed Procedure contracts or `ContractHash` binding;
- define SRPL syntax, parser behavior, or typed IR lowering;
- define durable WAL bytes, page formats, or RPC frame bytes;
- serialize Rust native structs directly to disk or network;
- treat RAM, temporary files, audit records, GPU output, or benchmark output as
  catalog truth;
- expose Administration or HA/DR capabilities through the Application surface;
- claim that a dedicated `CatalogDiff` runtime type already exists.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/specs/CatalogObjectModel_v0.md`
- `documentations/specs/DefinitionBatch_v0.md`
- `documentations/specs/ProcedureContract_v0.md`
- `documentations/specs/ContractCompatibility_v0.md`
- `documentations/specs/ContractHash_Canonicalization_v0.md`
- `documentations/specs/PlanCacheKey_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `crates/andromeda-catalog/src/batch/definition.rs`
- `crates/andromeda-catalog/src/batch/mutation.rs`
- `crates/andromeda-contract/src/contracts/validation.rs`

## Procedure

### Ownership

`andromeda-catalog` owns catalog diff production, storage correlation, dry-run
correlation, publication impact, and recovery explanation.

`andromeda-contract` owns Procedure contract materialization, canonical
`ContractHash`, `PolicyVersion`, and current Procedure compatibility
diagnostics. Catalog diff consumers must treat those contract diagnostics as
contract-plane evidence and must not reimplement incompatible call-shape logic
with string comparisons.

### Diff inputs

A catalog diff compares a base catalog state with a target catalog state.

| Input | Required rule |
| --- | --- |
| Base `CatalogVersion` | Must be non-zero when reading a published state and must match the state being diffed. |
| Target `CatalogVersion` | Must be strictly greater than the base version for publication diffs. |
| Object version records | Must include stable `CatalogObjectId`, object kind, qualified name, version range, and publication state. |
| Procedure contract evidence | Must include old and new `ContractHash` values when a Procedure version changes. |
| Binding evidence | Must include `ProcedureContractBinding` when invocation, admission, plan-cache, or audit evidence is affected. |
| Dependency evidence | Must include required object ids, version ranges, edge kinds, and dependency validation result. |
| Batch evidence | Must include `DefinitionBatchId`, operation index, source hash, dependency graph hash, and dry-run result when diffed from a batch. |
| WAL evidence | Must include durable catalog mutation evidence before a publication diff is accepted as visible truth. |

Names may appear in operator-facing diagnostics, but identity comparison must
use durable object ids and version ids as truth.

### Object version diff record

Each diff entry must be bounded and deterministic.

| Field | Required content |
| --- | --- |
| `object_id` | Stable non-zero object identity. |
| `object_kind` | Stable object kind such as Procedure, Table, StructuredObject, Enum, Map, policy, or statistics descriptor. |
| `previous_version` | Optional previous object version evidence. Missing only for create/additive introduction. |
| `next_version` | Optional next object version evidence. Missing only for deprecation or rejected removal evidence. |
| `previous_name` | Previous qualified name when the object existed. |
| `next_name` | Next qualified name when the object remains active. |
| `previous_contract_hash` | Previous Procedure `ContractHash` when the object is a Procedure. |
| `next_contract_hash` | Next Procedure `ContractHash` when the object is an active Procedure after the diff. |
| `previous_policy_version` | Previous `PolicyVersion` when policy impact is known. |
| `next_policy_version` | Next `PolicyVersion` when policy impact is known. |
| `previous_stats_version` | Previous `StatsVersion` when plan or admission impact is known. |
| `next_stats_version` | Next `StatsVersion` when plan or admission impact is known. |
| `dependency_delta` | Added, removed, changed, unchanged, or rejected dependency edges. |
| `lifecycle_class` | `Unchanged`, `Additive`, `Breaking`, `Deprecated`, or `Rejected`. |
| `compatibility_diagnostics` | Stable diagnostics that justify the classification. |
| `plan_cache_impact` | Required invalidation or no-op evidence. |
| `audit_correlation` | Batch, operation index, WAL, and publication correlation when available. |

The diff record must not include raw credentials, raw private keys, unbounded
source text, host-specific absolute paths, memory addresses, debug formatting,
or native struct layout bytes.

### Lifecycle classes

Catalog diff lifecycle classes describe how a catalog object version transition
affects consumers.

| Class | Meaning | Default visibility effect |
| --- | --- | --- |
| `Unchanged` | The object is visible in both states and all relevant identity, contract, dependency, policy, and statistics evidence is unchanged. | Existing consumers remain bound to the same evidence. |
| `Additive` | The target state adds a new object or adds compatible contract surface without invalidating existing callers. | New callers may bind to the target version; existing compatible callers remain valid. |
| `Breaking` | The target state changes identity, call shape, dependency requirements, permissions, policy, or visibility in a way that existing callers cannot assume is safe. | New binding or explicit migration is required. |
| `Deprecated` | The target state ends active visibility for an object while preserving historical evidence. | New invocations must not bind to the deprecated version; historical lookup and recovery evidence remain available. |
| `Rejected` | Validation, compatibility, dependency, or durability checks failed before publication. | No catalog visibility changes. |

Deprecation is a lifecycle transition, not physical erasure. A deprecated
object version remains available for recovery, audit review, historical
diagnostics, and compatibility analysis.

### Additive changes

A diff may classify a change as additive only when the existing consumer
contract remains valid and the change is explicitly accepted by policy.

Examples:

| Change | Required evidence |
| --- | --- |
| New object creation | No active name conflict, valid object identity, visible dependencies, and accepted DefinitionBatch evidence. |
| New Procedure with unused name | Valid `ProcedureContract`, canonical `ContractHash`, required permissions, and no conflict. |
| Procedure result extension | `CompatibilityPolicy::AdditiveOnly`, same Procedure identity, advancing `CatalogVersion`, and accepted contract diagnostic. |
| Added result stream column | Existing result stream columns remain an unchanged prefix and all non-output policy fields remain unchanged. |
| New dependency that does not affect existing callers | Dependency is visible, compatible, and does not widen Application-surface authority. |
| Statistics metadata advancement | Bounded, versioned, observable, explainable, disableable, and not treated as truth. |

The current implemented additive Procedure rule is intentionally narrow:
`andromeda-contract` accepts appended result stream columns under
`AdditiveOnly` while rejecting input, structured input, permission, protocol,
transaction, result metadata, error, multi-result, stats, stream removal,
stream id, and stream cardinality changes.

### Breaking changes

A diff must classify a change as breaking when existing consumers cannot safely
continue with the previous evidence.

Examples:

| Change | Required behavior |
| --- | --- |
| Procedure identity drift | Reject as breaking when `ProcedureId`, `CatalogObjectId`, name, or object kind changes without explicit identity-preserving evidence. |
| Non-advancing version | Reject as breaking because compatibility cannot be classified without an advancing `CatalogVersion`. |
| Exact-hash mismatch | Reject as breaking under `CompatibilityPolicy::ExactHash`. |
| Input or structured input change | Reject as breaking because invocation shape changed. |
| Required permission change | Reject as breaking because security admission changed. |
| Transaction, result metadata, error, multi-result, protocol, policy, or stats change | Reject as breaking unless a future reviewed compatibility rule explicitly accepts it. |
| Result stream removal, id change, cardinality change, or existing column reorder | Reject as breaking. |
| Dependency removal or incompatible dependency version | Reject or require explicit migration evidence. |
| Rename or move | Treat as breaking unless identity-preserving evidence and dependency update validation are present. |

Breaking does not mean the target state is always illegal. It means publication,
client binding, plan-cache reuse, and rollback analysis require explicit
migration evidence and diagnostics.

### Deprecated changes

A diff classifies an object as deprecated when a durable lifecycle transition
ends active visibility.

Required evidence:

| Evidence | Rule |
| --- | --- |
| Target object | Must identify a visible object version at or before the batch base version. |
| Deprecation version | Must equal the planned target catalog version. |
| Dependency closure | Active dependents must be absent or explicitly handled by policy. |
| Invocation policy | New invocation admission must reject the deprecated object version. |
| Historical retention | Previous object version evidence must remain available for recovery and audit. |
| WAL evidence | Publication must not be visible before durable WAL covers the deprecation. |

Deprecation must invalidate or fence cached plans that bind to the deprecated
object version for new invocations. Existing in-flight behavior must follow the
transaction and invocation policy defined by the catalog owner.

### Diff algorithm

Implementations must compute diffs deterministically.

1. Read the base and target catalog state by durable identity.
2. Validate both catalog states before comparing them.
3. Index object versions by `CatalogObjectId` and object kind.
4. Match object versions by durable identity, not by qualified name.
5. Detect creations, version replacements, deprecations, renames, moves, and
   rejected operations.
6. For Procedure replacements, validate canonical hashes and run contract
   compatibility diagnostics.
7. Compare dependency edges by durable object id, version range, and edge kind.
8. Compare `StatsVersion` and `PolicyVersion` when they participate in
   admission, planning, or audit evidence.
9. Classify each diff entry using the lifecycle rules in this spec.
10. Produce stable diagnostics for every rejected or breaking entry.
11. Sort entries by object kind tag, object id, operation index when present,
    and qualified name as a final diagnostic tie-breaker.

The algorithm must not depend on hash-map iteration order, thread scheduling,
timestamps, random values, memory addresses, debug formatting, or host-specific
paths.

### Compatibility diagnostics

Catalog diff diagnostics must be stable, typed, sanitized, and tied to the diff
entry and operation index when available.

| Diagnostic code | Meaning | Required evidence |
| --- | --- | --- |
| `CDIFF-IDENTITY-DRIFT` | A matched object changed durable identity evidence unexpectedly. | Object id, old evidence, new evidence, operation index. |
| `CDIFF-VERSION-NON-ADVANCING` | Target object or catalog version did not advance. | Previous version, target version, object id. |
| `CDIFF-CONTRACT-HASH-CHANGED` | Procedure contract hash changed. | Old hash, new hash, compatibility policy. |
| `CDIFF-COMPATIBILITY-REJECTED` | Contract or catalog compatibility rejected the transition. | Compatibility class, reason code, contract diagnostic. |
| `CDIFF-DEPENDENCY-REMOVED` | A dependency edge disappeared or became incompatible. | Edge kind, dependent id, dependency id, required version range. |
| `CDIFF-DEPRECATED` | Object active visibility ended. | Object id, deprecated version, dependency closure result. |
| `CDIFF-WAL-NOT-DURABLE` | Diff was treated as visible before durable WAL evidence. | Required LSN, observed durable LSN, batch id. |
| `CDIFF-UNSUPPORTED-CHANGE` | The diff engine saw a change class it cannot classify. | Object kind, operation kind, operation index. |

Diagnostics may include bounded SRPL spans or catalog source correlation when
the source is operator-selected. They must not include raw secrets, unbounded
source text, raw credentials, private keys, or ambient absolute paths.

### Plan-cache and admission impact

Catalog diffs must report plan-cache and admission impact explicitly.

| Diff class | Plan-cache rule | Admission rule |
| --- | --- | --- |
| `Unchanged` | Existing keys may remain valid when all bound identities match. | Existing bindings remain valid. |
| `Additive` | Existing keys may remain valid only when the bound `ContractHash`, `CatalogVersion`, `StatsVersion`, and `PolicyVersion` policy allow reuse. New keys bind the new evidence. | Existing compatible bindings may continue; new callers bind target evidence. |
| `Breaking` | Invalidate or fence affected keys. Reuse requires a new key and explicit migration evidence. | Reject old or mismatched bindings before transaction creation. |
| `Deprecated` | Fence keys for new invocations of the deprecated version. | Reject new invocation binding to the deprecated active name or object version. |
| `Rejected` | No accepted catalog state exists; no cache mutation may be treated as truth. | Reject the operation and preserve diagnostics. |

## Validation

Documentation acceptance checks:

- The spec states that catalog diff is evidence and not database truth.
- The spec uses durable object identity and version evidence rather than names
  as truth.
- The spec defines additive, breaking, deprecated, unchanged, and rejected
  lifecycle classes.
- The spec requires Procedure contract compatibility diagnostics for Procedure
  version replacements.
- The spec requires deterministic ordering and sanitized diagnostics.
- The spec does not introduce ad hoc SQL, runtime JSON defaults, native Rust
  struct serialization, or GPU work in critical paths.

Current focused implementation evidence:

```powershell
cargo test -p andromeda-catalog --test catalog_diff_contract
```

Future implementation work should add or keep targeted validation for:

```powershell
cargo test -p andromeda-catalog --test catalog_diff_contract
cargo test -p andromeda-catalog --test batch_alter_drop_compat
cargo test -p andromeda-catalog --test catalog_publication_subscription
cargo test -p andromeda-catalog --test plan_invalidation
cargo test -p andromeda-storage --test catalog_recovery_replay_contract
```

### Validation matrix

| Scenario | Expected result | Evidence |
| --- | --- | --- |
| New Procedure object appears with no conflict. | Classified as additive. | DefinitionBatch dry-run and source-hash tests. |
| Procedure result columns append under `AdditiveOnly`. | Classified as additive when existing columns remain an unchanged prefix. | `ProcedureContract::compatibility_with()` tests. |
| Procedure input changes. | Classified as breaking with compatibility diagnostics. | Contract compatibility tests. |
| Procedure `ContractHash` changes under `ExactHash`. | Classified as breaking. | Contract compatibility tests. |
| Object is deprecated. | Classified as deprecated with historical retention evidence. | DefinitionBatch deprecate dry-run tests. |
| Create and deprecate target same object in one implicit operation set. | Rejected before publication. | DefinitionBatch lifecycle conflict tests. |
| Diff attempts visibility before durable WAL. | Rejected with durability diagnostic. | Future WAL fence tests. |

## Current Implementation Status

Implemented:

- `andromeda-catalog` exposes DefinitionBatch dry-run, source hash,
  dependency graph hash, mutation plans, create deltas, and deprecate deltas.
- `andromeda-catalog` reexports Procedure contract compatibility types from
  `andromeda-contract`.
- `andromeda-contract` implements current Procedure compatibility diagnostics
  through `ProcedureContract::compatibility_with()`.
- Existing plan-cache tests verify that `CatalogVersion`, `ContractHash`,
  `StatsVersion`, and `PolicyVersion` participate in plan identity.

Pending or partial:

- A dedicated `CatalogDiff` runtime type and durable diff codec are not yet
  implemented.
- Diff diagnostics are documented here as stable target behavior; current
  contract compatibility diagnostics are string messages rather than typed
  diagnostic records.
- Full publication, WAL durability, recovery, and subscription integration for
  catalog diffs remains future implementation work.
- Rename, move, replace, and explicit alter operation classes are still roadmap
  work beyond the current create and deprecate DefinitionBatch surface.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Diff treats a rename as a new unrelated object. | The implementation matched objects by name instead of durable object id. | Match by `CatalogObjectId` and report name changes as diff evidence. |
| Additive diff accepts an input change. | Compatibility classification ignored invocation shape. | Run Procedure contract compatibility diagnostics and reject as breaking. |
| Deprecated object disappears from history. | Deprecation was implemented as physical erasure. | Preserve object version evidence and publish only a lifecycle transition. |
| Plan cache reuses a stale key after a breaking diff. | Diff did not report cache impact. | Bind invalidation to `CatalogVersion`, `ContractHash`, `StatsVersion`, and `PolicyVersion`. |
| Diff output changes order between runs. | Ordering depends on map iteration or source collection order. | Sort by stable object kind, object id, operation index, and qualified name. |
| Diff claims visibility before WAL durability. | Dry-run or audit evidence was treated as truth. | Require durable WAL and publication evidence before visible catalog state changes. |

## References

- `documentations/specs/CatalogObjectModel_v0.md`
- `documentations/specs/DefinitionBatch_v0.md`
- `documentations/specs/ProcedureContract_v0.md`
- `documentations/specs/ContractCompatibility_v0.md`
- `documentations/specs/ContractHash_Canonicalization_v0.md`
- `documentations/specs/PlanCacheKey_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `crates/andromeda-catalog/src/batch/definition.rs`
- `crates/andromeda-catalog/src/batch/mutation.rs`
- `crates/andromeda-contract/src/contracts/validation.rs`
- `crates/andromeda-catalog/tests/catalog_diff_contract.rs`
