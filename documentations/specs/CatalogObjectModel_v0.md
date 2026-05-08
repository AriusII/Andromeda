# CatalogObjectModel v0 Specification

## Purpose

Define the accepted documentation contract for `CatalogObjectModel v0`, the
catalog identity, version, dependency, and publication model used by Andromeda
typed Procedure contracts.

The catalog is durable engine metadata. It is not an application-facing SQL
surface, not a runtime JSON registry, and not a source of visible truth until
the associated catalog mutation has durable WAL evidence.

## Scope

This specification applies to catalog object identity, object versioning,
contract hash binding, dependency edges, compatibility decisions, monotonic
catalog publication, and recovery evidence.

It covers:

- stable catalog object identities;
- namespace and qualified-name uniqueness rules;
- object version records and version state transitions;
- `ContractHash` binding for typed Procedure contracts;
- dependency graph requirements;
- monotonic publication from candidate catalog state to active catalog state;
- WAL and recovery evidence required before visibility;
- diagnostics and validation criteria for documentation and future code work;
- current crate ownership and pending crate extraction status.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL, generic command text, dynamic table
  names, or dynamic predicates;
- bypass typed Procedure contracts;
- define SRPL syntax, parsing, binding, or lowering;
- define Protobuf message schemas or RPC frame layouts;
- define physical page, heap, B-tree, segment, or cold snapshot formats;
- serialize Rust native structs directly to disk or network;
- make audit records, RAM state, temp files, GPU output, or benchmark output
  database truth;
- claim that catalog crate extraction is complete;
- replace the WAL, recovery, security, or RPC specifications that own their
  respective durable boundaries.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- DEC-029 for catalog WAL record design and recovery replay requirements.
- DEC-030 for SRPL to DefinitionBatch integration and all-or-nothing behavior.
- `documentations/specs/DefinitionBatch_v0.md` for batch ordering, dry-run, and
  apply semantics.
- `documentations/specs/AuditLedger_v0.md` for catalog decision audit evidence
  boundaries.
- `documentations/specs/SecurityAdmission_v0.md` for contract and catalog
  evidence required before Procedure invocation.
- `crates/README.md` for current crate ownership and compatibility facade
  notes.

## Procedure

### Ownership

`andromeda-catalog` owns catalog storage, DefinitionBatch behavior, plan cache
identity, statistics metadata, publication, and WAL-facing catalog codecs.

`andromeda-contract` owns contract-safe Procedure contracts, qualified names,
catalog object descriptors, and structural catalog dependency edges.

Catalog crate extraction is still pending. Until the extraction is complete,
documentation and code must describe `andromeda-catalog` as the current catalog
engine owner and must not claim a dedicated extracted catalog object model crate
as an accepted runtime boundary.

### Identity model

Catalog identity is stable and typed. Names are human-facing selectors. Object
identifiers and version identifiers are durable identity evidence.

| Field | Rule |
| --- | --- |
| `CatalogObjectId` | Stable nonzero identity for a catalog object across compatible versions. |
| `CatalogObjectKind` | Bounded object kind such as Procedure, namespace, Map, type descriptor, statistics descriptor, or policy descriptor. |
| `NamespaceId` | Stable nonzero identity for a namespace. |
| `QualifiedName` | Human-facing path scoped by namespace. Active names must be unique within the namespace and object kind policy. |
| `CatalogVersion` | Monotonic catalog publication version. It advances only through durable publication. |
| `CatalogObjectVersionId` | Stable identity for a specific object version record. |
| `ContractHash` | Deterministic hash over the canonical typed contract representation. |
| `DefinitionBatchId` | Correlation identity for the batch that created, altered, deprecated, or restored a version. |

Rules:

1. A visible object must have a nonzero `CatalogObjectId`.
2. A visible object version must have a nonzero `CatalogObjectVersionId`.
3. A visible Procedure object version must bind exactly one `ContractHash`.
4. A name lookup resolves to an object version at a specific
   `CatalogVersion`.
5. A direct object-id lookup resolves only when the selected object version is
   visible at the requested `CatalogVersion`.
6. Renaming, moving, or aliasing an object requires explicit catalog evidence.
   It must not be represented as an implicit string rewrite.
7. Dropping an object must create durable deprecation evidence. It must not
   erase prior object version history required for recovery, audit, or
   compatibility review.

### Object version record

Each catalog object version record must carry enough evidence to validate the
object independently of mutable in-memory state.

| Evidence | Required content |
| --- | --- |
| Identity | `CatalogObjectId`, `CatalogObjectVersionId`, `CatalogObjectKind`, namespace, qualified name. |
| Version range | Introduced catalog version, optional deprecated catalog version, publication state. |
| Contract binding | `ContractHash`, contract schema version, compatibility class when applicable. |
| Dependencies | Required object ids, required version ranges, dependency edge kinds, optional policy and statistics versions. |
| Batch evidence | `DefinitionBatchId`, operation kind, dry-run report hash, applied operation index. |
| WAL evidence | Catalog WAL record kind, record LSN, durable LSN, payload checksum or chain evidence as owned by WAL. |
| Recovery evidence | Replay order, replay status, checkpoint correlation when available. |
| Diagnostic evidence | Stable diagnostic code, severity, source correlation when applicable, sanitized message. |

Object version records must be encoded by explicit canonical codecs when they
become durable storage or network payloads. Rust native struct layout is never
the durable or wire contract.

### Version states

Catalog object versions move through bounded states:

| State | Meaning | Visibility |
| --- | --- | --- |
| Candidate | Constructed by a DefinitionBatch before validation. | Not visible. |
| Validated | Dry-run accepted the object and its dependencies. | Not visible. |
| WalRecorded | Apply emitted deterministic catalog WAL evidence. | Not visible until durable WAL is confirmed. |
| Published | The active catalog pointer advanced to include this version. | Visible at and after the publication version, subject to deprecation. |
| Deprecated | A later durable catalog version ended active visibility. | Historical lookup only. |
| Rejected | Validation, compatibility, or durability failed. | Never visible. |

Publication must fail closed if a state transition is missing, duplicated, or
observed out of order.

### Dependency graph

The catalog dependency graph records typed edges between object versions.

| Edge kind | Required validation |
| --- | --- |
| Procedure contract dependency | Referenced Procedure exists, has a visible compatible version, and has the expected `ContractHash` when pinned. |
| Map dependency | Source Procedure, type, and statistics requirements are visible at the selected `CatalogVersion`. |
| Type dependency | Referenced type descriptor exists and satisfies nullability, scalar, enum, flags, decimal, text, or structured-object policy. |
| Policy dependency | Required permission or policy version is visible and compatible. |
| Statistics dependency | Required `StatsVersion` is visible and not newer than the catalog state being published unless an explicit policy allows it. |

Dependency validation must use object ids and version ranges as truth. Names may
be used for diagnostics and operator review, but names must not replace durable
identity evidence.

### Contract hash binding

`ContractHash` is the stable binding between a cataloged Procedure version and
the typed Procedure contract accepted by RPC, security admission, execution, and
plan cache gates.

The hash input must be canonical, bounded, and deterministic. It must include
the contract fields that affect call shape, input and output types, result
cardinality, permission requirements, relevant compatibility metadata, and
versioned policy references. It must not include timestamps, memory addresses,
map iteration order, debug formatting, random values, host-specific paths, or
native struct padding.

Any contract-affecting change creates a new object version. Whether that change
is compatible is a separate explicit decision and must be recorded as catalog
compatibility evidence.

### Monotonic publication

Catalog publication is monotonic. The active catalog pointer may move only from
the current accepted `CatalogVersion` to a strictly greater version produced by
a validated DefinitionBatch.

Publication sequence:

1. Read the current active `CatalogVersion`.
2. Build a candidate catalog state from the current active state plus the
   DefinitionBatch operations.
3. Validate identity, name uniqueness, dependency graph, compatibility, and
   diagnostics.
4. Emit deterministic catalog WAL records for the accepted mutation set.
5. Confirm durable WAL coverage for the catalog mutation records.
6. Atomically publish the new active catalog version.
7. Emit subscription, audit, and recovery evidence that references the durable
   LSN and the new `CatalogVersion`.

No caller may observe a candidate object version, a validated-but-not-durable
object version, or a WAL-recorded-but-not-published object version as active.

Publication gaps are allowed only when the implementation records why the gap
exists and recovery can replay the same active version sequence. Version
rollback is not allowed during normal operation. Recovery may select an earlier
valid snapshot or checkpoint only as part of explicit startup or restore
procedure evidence.

### WAL and recovery evidence

Catalog publication is accepted only when durable WAL evidence proves the
mutation sequence.

Required evidence:

| Evidence | Rule |
| --- | --- |
| Record kind | Catalog WAL record kind identifies DefinitionBatch apply, object add, object alter, object drop, statistics update, or checkpoint. |
| LSN | Each record is ordered by WAL LSN. |
| Durable LSN | Publication must not become visible before durable coverage includes the catalog mutation. |
| Payload checksum | Recovery must reject corrupted or truncated catalog records. |
| Replay order | Recovery replays catalog records in LSN order and enforces monotonic `CatalogVersion`. |
| Checkpoint | Checkpoints summarize a durable catalog boundary and must match replayed object counts and version evidence. |
| Recovery report | Startup and restore reports must explain accepted, rejected, skipped, and corrupted catalog records. |

Audit evidence may describe catalog decisions, but audit evidence is forensic
evidence only. Database truth remains the latest valid cold snapshot plus
durable WAL from that snapshot.

### Compatibility

Compatibility is explicit catalog evidence, not an inference from a matching
name or a reused object id.

| Change | Default compatibility rule |
| --- | --- |
| Add new Procedure | Compatible with prior catalog versions when no name or dependency conflict exists. |
| Alter Procedure without call-shape change | May be compatible only when the compatibility policy records the accepted class. |
| Alter Procedure with input, output, cardinality, permission, or policy change | Requires explicit compatibility review and a new `ContractHash`. |
| Drop Procedure | Incompatible for new invocations and allowed only when dependencies and active invocation policy permit it. |
| Rename or move | Requires explicit identity-preserving evidence and dependency update validation. |
| Statistics update | Compatible with catalog identity only when bounded, versioned, observable, and disableable. |

Compatibility decisions must preserve older object version evidence needed for
recovery, audit review, rollback analysis, and plan cache invalidation.

### Diagnostics

Catalog diagnostics must be stable, typed, sanitized, and correlated with the
batch or object version that produced them.

| Diagnostic code | Meaning | Required evidence |
| --- | --- | --- |
| `CAT-IDENTITY-ZERO` | An object, namespace, version, or batch id is zero. | Field name and operation index. |
| `CAT-NAME-CONFLICT` | A qualified name conflicts with an active or candidate object. | Namespace, object kind, conflicting object id. |
| `CAT-VERSION-NON-MONOTONIC` | A catalog version did not strictly advance. | Current version, attempted version, operation index. |
| `CAT-CONTRACT-HASH-MISMATCH` | The supplied hash does not match canonical contract bytes. | Expected hash, actual hash, object id. |
| `CAT-DEPENDENCY-MISSING` | A required dependency is absent or not visible. | Dependency edge kind, required id or name, version range. |
| `CAT-COMPATIBILITY-REJECTED` | The compatibility policy rejected an object change. | Compatibility class, reason code, object id. |
| `CAT-WAL-NOT-DURABLE` | Publication was attempted before durable WAL coverage. | Required LSN and observed durable LSN. |
| `CAT-RECOVERY-REPLAY-REJECTED` | Recovery rejected a catalog record or sequence. | LSN, record kind, reason code. |

Diagnostic messages must not include raw secrets, raw credentials, unbounded
source text, or host-specific absolute paths unless the path is already part of
an explicit operator-selected diagnostic context.

## Validation

Documentation acceptance checks:

- The spec states that catalog publication is monotonic.
- The spec states that visible publication requires durable WAL evidence.
- The spec separates names from durable object identity.
- The spec binds Procedure versions to `ContractHash`.
- The spec preserves dependency graph and compatibility evidence.
- The spec states that catalog crate extraction is still pending.
- The spec does not introduce ad hoc SQL, gRPC, runtime JSON defaults, or native
  Rust struct layout serialization.
- The spec does not treat audit records, RAM state, temp files, GPU output, or
  benchmark output as database truth.

Future implementation work should add or keep targeted validation for:

```powershell
cargo test -p andromeda-catalog --test catalog_object_identity_contract
cargo test -p andromeda-catalog --test catalog_publication_monotonicity
cargo test -p andromeda-catalog --test catalog_dependency_graph_contract
cargo test -p andromeda-storage --test catalog_wal_contract
cargo test -p andromeda-storage --test catalog_recovery_replay_contract
```

These commands are not required for WR12-D documentation-only acceptance.

### Validation matrix

| Area | Acceptance criterion | Evidence |
| --- | --- | --- |
| Identity | Object ids and object version ids are nonzero and stable. | Unit and property tests for identity constructors and catalog records. |
| Names | Active qualified names are unique within their namespace policy. | Duplicate-name dry-run and publication tests. |
| Versions | `CatalogVersion` advances monotonically and never rolls back during normal publication. | Monotonic publication tests and recovery replay tests. |
| Contracts | Procedure object versions bind deterministic `ContractHash` values. | Canonical contract hash tests and mismatch diagnostics. |
| Dependencies | Missing, incompatible, or cyclic dependencies are rejected before apply. | Dependency graph tests and diagnostic snapshots. |
| WAL | Publication is blocked until durable WAL coverage includes catalog records. | WAL fence tests and crash-injection tests. |
| Recovery | Replay reconstructs the same active catalog state or reports a corruption boundary. | Recovery replay and checkpoint consistency tests. |
| Compatibility | Contract-affecting changes require explicit compatibility evidence. | Alter, drop, and plan cache invalidation tests. |
| Diagnostics | Failures produce stable sanitized diagnostic codes. | Diagnostic contract tests. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A candidate object is visible to callers. | Publication state leaked before durable publication. | Block visibility until WAL coverage and active pointer advancement complete. |
| Recovery sees catalog versions out of order. | WAL records are missing, duplicated, or replayed out of LSN order. | Stop at the corruption boundary and preserve a recovery report. |
| A Procedure resolves by name but the contract hash differs. | Name lookup bypassed contract binding evidence. | Reject with `CAT-CONTRACT-HASH-MISMATCH` and require a compatible catalog version. |
| A drop breaks a dependent Map or Procedure. | Dependency validation ignored active edges. | Reject the batch during dry-run and report `CAT-DEPENDENCY-MISSING` or compatibility failure. |
| Documentation claims a dedicated catalog object model crate exists. | Crate extraction wording drifted ahead of implementation. | Reword to pending extraction and current `andromeda-catalog` ownership. |
| Audit evidence is described as catalog truth. | Forensic evidence was confused with durable storage truth. | Reword to latest valid snapshot plus durable WAL truth. |

## References

- `documentations/governance/decisions/DEC-029-e4-catalog-wal.md`
- `documentations/governance/decisions/DEC-030-e7-srpl-definitionbatch.md`
- `documentations/specs/DefinitionBatch_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `crates/README.md`
