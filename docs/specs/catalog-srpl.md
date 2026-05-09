# Catalog And SRPL

## Purpose

This spec defines catalog object identity, DefinitionBatch validation and
publication, catalog diffs, and the SRPL-to-contract bridge. Catalog state may
become visible only after validation, compatibility checks, dependency checks,
and durable catalog WAL evidence are complete.

## Ownership

The catalog owner controls catalog storage, DefinitionBatch, snapshots,
publication, Procedure Store integration, statistics metadata, plan-cache
identity, and catalog WAL integration. The contract owner controls Procedure
contracts and compatibility diagnostics. SRPL model crates own syntax and
language-model contracts; the SRPL facade may preserve historical compiler and
catalog bridge entry points until the bridge has a dedicated owner.

## Catalog identity

Catalog identity is stable and typed. Names are human-facing selectors, not
durable identity.

| Evidence | Rule |
| --- | --- |
| Object id | Nonzero, stable, and never derived from display name alone. |
| Object kind | Procedure, table, StructuredObject, enum, map, policy, statistics descriptor, or another accepted kind. |
| Qualified name | Unique among active objects in the namespace policy. |
| Catalog version | Nonzero and strictly advancing for publication. |
| Object version | Records lifecycle state, dependency evidence, contract evidence, WAL evidence, and recovery evidence. |

Publication is monotonic. The active catalog pointer may move only from an
accepted version to a strictly newer accepted version. Normal operation must
not roll the active pointer backward. Recovery may select an earlier version
only when snapshot, WAL, and recovery evidence require it.

## Object lifecycle

| State | Meaning | Visibility |
| --- | --- | --- |
| `Candidate` | Parsed or built but not validated. | Not visible. |
| `Validated` | Identity, dependency, compatibility, and policy checks passed. | Not visible until WAL is durable. |
| `WalRecorded` | Deterministic catalog WAL records exist. | Not visible until durable WAL coverage is confirmed. |
| `Published` | Active catalog pointer advanced after durable evidence. | Visible for binding according to policy. |
| `Deprecated` | Active visibility ended while historical evidence remains. | New invocations must not bind to it. |
| `Rejected` | Validation or durability failed. | Never visible. |

Recovery replays catalog records in LSN order, enforces monotonic
`CatalogVersion`, and reports accepted, rejected, skipped, and corrupted
records.

## DefinitionBatch

A DefinitionBatch is the only accepted batch surface for catalog mutations in
this spec.

| Field | Rule |
| --- | --- |
| `DefinitionBatchId` | Stable nonzero batch identity. |
| Base `CatalogVersion` | Active catalog version used as the validation source. |
| Target `CatalogVersion` | Strictly greater version to publish if apply succeeds. |
| Operations | Ordered candidate operations with stable operation indexes. |
| Principal evidence | Required for administration or system-initiated catalog changes. |
| Dry-run report hash | Deterministic hash of bounded validation results. |
| WAL evidence | Catalog WAL records, LSN range, durable LSN, and payload checksum evidence. |
| Recovery evidence | Replay status, rejected operation evidence, and checkpoint correlation when available. |

Dry-run is pure. It must not mutate active catalog state, emit WAL, advance
`CatalogVersion`, publish statistics, or affect plan-cache truth. It produces
bounded deterministic evidence.

Apply is all-or-nothing. No operation from a partially failed batch may become
visible. Visible publication requires durable WAL coverage for the complete
accepted batch.

## Operation validation

| Operation | Required checks |
| --- | --- |
| Create | Name availability, valid identity, deterministic contract hash, visible dependencies. |
| Alter | Existing target, explicit identity preservation, compatibility acceptance. |
| Drop or deprecate | Existing target, dependency closure, active invocation policy, historical evidence retention. |
| Rename or move | Preserved object id, namespace conflict check, dependency updates, compatibility evidence. |
| Update descriptor | Monotonic descriptor version and canonical hash evidence. |
| Update statistics metadata | Bounded, versioned, observable, explainable, disableable, and not treated as truth. |

Conflict detection must reject duplicate active names, create-and-drop of the
same object without explicit replace semantics, multiple unsequenced alters of
the same object, missing dependencies, dependency cycles, non-monotonic target
versions, unsupported object kinds, and missing security or surface evidence.

Dependency ordering must be deterministic. Ties are resolved by stable
operation index and durable identity fields, never by map iteration order.

## Catalog diffs

Catalog diffs compare a base version with a strictly newer target version.
They must carry object identity, old and new contract hashes when a Procedure
changes, binding evidence, dependency evidence, batch evidence, and durable WAL
evidence before a diff is accepted as visible truth.

| Diff class | Plan-cache rule | Admission rule |
| --- | --- | --- |
| Unchanged | Existing keys may remain valid when all bound identities match. | Existing bindings remain valid. |
| Additive | Existing keys may remain valid only when contract, catalog, stats, and policy evidence allow reuse. | Existing compatible bindings may continue; new callers bind target evidence. |
| Breaking | Invalidate or fence affected keys. | Reject old or mismatched bindings before transaction creation. |
| Deprecated | Fence keys for new invocations of the deprecated version. | Reject new invocation binding to the deprecated active name or version. |
| Rejected | No accepted catalog state exists. | Reject the operation and preserve diagnostics. |

Input changes, required permission changes, result stream removal, id changes,
cardinality changes, existing column reorder, policy changes, stats identity
changes, protocol layout changes, and dependency removals are breaking unless
a reviewed compatibility rule explicitly accepts the transition.

## SRPL bridge

SRPL producers must lower to typed contract and catalog evidence. SRPL source
digest is retained for provenance, while `ContractHash` is computed from the
canonical typed Procedure shape. Equivalent formatting may change source digest
without changing `ContractHash`; semantic shape changes must change it.

SRPL model crates must stay free of catalog store, execution runtime, storage,
transport runtime, benchmark, and GPU dependencies. Catalog-facing bridge code
must remain explicit and must not hide catalog-store coupling inside parser or
language-model crates.

Ad hoc SQL is not a catalog or invocation surface. Published Procedure
contracts and typed SRPL-derived DefinitionBatch evidence are the accepted
contract path.

## Diagnostics

Catalog and DefinitionBatch diagnostics must be stable, sanitized, and tied to
operation indexes when applicable. Required codes include identity-zero,
duplicate-name, version-non-monotonic, dependency-missing, dependency-cycle,
contract-hash-mismatch, compatibility-rejected, WAL-not-durable, and
recovery-rejected variants.

## Validation gates

- Dry-run purity tests must prove no catalog mutation, WAL emission, or
  version advancement.
- Publication tests must prove no catalog visibility before durable WAL.
- Recovery tests must reject incomplete or corrupted batch WAL sequences.
- Compatibility tests must classify alters, drops, renames, moves, policy
  changes, and statistics changes explicitly.
- Diagnostics must not include raw source or secret-bearing values.

## Open decisions

- Dedicated catalog object model and DefinitionBatch crates remain pending
  until ownership, topology, and owner tests are accepted.
- A dedicated SRPL catalog bridge owner remains pending.
