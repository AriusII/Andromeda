# DefinitionBatch v0 Specification

## Purpose

Define the accepted documentation contract for `DefinitionBatch v0`, the
catalog mutation unit that validates, orders, persists, and publishes typed
Procedure and catalog object definitions.

A DefinitionBatch is all-or-nothing. It may create candidate catalog state, but
no part of the batch becomes visible until validation succeeds, deterministic
catalog WAL evidence is durable, and catalog publication advances monotonically.

## Scope

This specification applies to DefinitionBatch construction, dependency
ordering, dry-run validation, apply behavior, WAL and recovery evidence,
compatibility decisions, diagnostics, and validation matrices.

It covers:

- batch identity and operation model;
- supported operation classes;
- dependency ordering and conflict detection;
- dry-run reports and side-effect boundaries;
- all-or-nothing apply behavior;
- monotonic publication through `CatalogVersion`;
- WAL and recovery evidence required for visibility;
- compatibility and diagnostics requirements;
- current crate ownership and pending crate extraction status.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL, generic command text, dynamic table
  names, dynamic predicates, or shape-shifting returns;
- bypass typed Procedure contracts;
- define SRPL grammar, parser internals, binder internals, or IR lowering
  details;
- define RPC, QUIC, Protobuf, or ResultStream wire contracts;
- define physical WAL segment, page, heap, or cold snapshot byte layouts;
- serialize Rust native structs directly to disk or network;
- place GPU work in validation, apply, WAL, rollback, recovery, MVCC
  visibility, catalog publication, or security-critical paths;
- claim that catalog or DefinitionBatch crate extraction is complete;
- claim production completeness for any runtime path without matching code and
  recovery evidence.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- DEC-029 for catalog WAL record design and replay requirements.
- DEC-030 for SRPL to DefinitionBatch integration and dry-run behavior.
- `documentations/specs/CatalogObjectModel_v0.md` for identity, version,
  dependency, and publication rules.
- `documentations/specs/AuditLedger_v0.md` for catalog decision audit evidence.
- `documentations/specs/SecurityAdmission_v0.md` for Procedure invocation
  contract evidence.
- `crates/README.md` for current crate ownership and compatibility facade
  notes.

## Procedure

### Ownership

`andromeda-catalog` owns DefinitionBatch behavior, catalog publication, and
WAL-facing catalog codecs in the current workspace.

SRPL compiler crates may provide parsed, bound, and lowered Procedure material
for batch validation, but they must not own catalog publication, WAL durability,
or active catalog visibility.

DefinitionBatch crate extraction is still pending. Until extraction is complete,
documentation and code must describe DefinitionBatch as current
`andromeda-catalog` behavior and must not claim a dedicated extracted
DefinitionBatch crate as an accepted runtime boundary.

### Batch model

A DefinitionBatch groups catalog mutation operations under one validation and
publication decision.

| Field | Rule |
| --- | --- |
| `DefinitionBatchId` | Stable nonzero identity for the batch. |
| Base `CatalogVersion` | Active catalog version used as the validation source. |
| Target `CatalogVersion` | Strictly greater version to publish if the batch applies. |
| Operation list | Ordered candidate operations with stable operation indexes. |
| Principal evidence | Operator or system principal correlation when required by the surface. |
| Dry-run report hash | Deterministic summary of validation results for accepted batches. |
| WAL evidence | Catalog WAL records, LSN range, durable LSN, payload checksum evidence. |
| Recovery evidence | Replay status, rejected operation evidence, checkpoint correlation when available. |

The operation list may be built from SRPL source, catalog descriptors, or
operator-controlled definition artifacts. Adding an operation to a batch must
not make the operation visible.

### Operation classes

DefinitionBatch v0 recognizes bounded catalog mutation classes:

| Operation class | Required validation |
| --- | --- |
| Create object | Name is available, identity is valid, contract hash is deterministic, dependencies exist. |
| Alter object | Target object exists, identity preservation is explicit, compatibility policy accepts the new version. |
| Drop object | Target object exists, active dependencies and invocation policy permit deprecation. |
| Rename or move object | Identity preservation, namespace conflict checks, dependency updates, and compatibility evidence are present. |
| Update descriptor | Descriptor version advances monotonically and canonical hash evidence matches. |
| Update statistics metadata | Statistics version is bounded, observable, explainable, disableable, and not treated as truth. |

Unsupported operation classes must be rejected during dry-run with stable
diagnostics. They must not fall through to apply.

### Dependency ordering

DefinitionBatch dry-run must build a dependency graph before apply. The graph
uses durable object identity and version ranges as truth. Names may appear in
diagnostics but must not replace object identity evidence.

Ordering rules:

1. Validate operation identities and operation indexes.
2. Normalize names and object selectors to candidate object identities.
3. Add dependency edges from each candidate object version to required object
   versions.
4. Add conflict edges for names, object ids, version ranges, contract hashes,
   and mutually exclusive operations.
5. Reject missing dependencies before topological ordering.
6. Reject cycles unless an explicit bounded rule permits the specific cycle.
7. Produce a deterministic topological order for apply.
8. Preserve original operation indexes in diagnostics even when apply order is
   topologically sorted.

Deterministic tie-breaking must use stable values such as operation index,
object id, object kind, and qualified name. It must not use hash-map iteration
order, memory addresses, thread scheduling, timestamps, or random values.

### Conflict detection

Dry-run must reject conflicts before apply.

| Conflict | Required behavior |
| --- | --- |
| Duplicate active name | Reject the batch. |
| Create and drop same object without explicit replace semantics | Reject the batch. |
| Multiple alters of same object without explicit sequencing | Reject the batch. |
| Contract hash mismatch | Reject the affected operation and the batch. |
| Dependency on dropped object | Reject unless the same batch creates a compatible replacement and policy permits it. |
| Non-monotonic target catalog version | Reject the batch. |
| Unsupported object kind or operation class | Reject the batch. |
| Missing permission, policy, or surface evidence | Reject before transaction creation or catalog publication. |

Conflict diagnostics must be accumulated where practical. A dry-run may stop
early only when continuing would produce misleading evidence, such as malformed
batch identity or unreadable canonical input.

### Dry-run behavior

Dry-run validates the whole batch without side effects.

Dry-run must:

1. Read the base catalog snapshot or active catalog view.
2. Validate batch identity and base `CatalogVersion`.
3. Canonicalize operation descriptors.
4. Compile or validate SRPL-derived Procedure material when the operation
   carries SRPL source.
5. Compute deterministic `ContractHash` values from canonical typed contracts.
6. Build and validate the dependency graph.
7. Evaluate compatibility policy for alters, drops, renames, moves, and
   descriptor updates.
8. Produce a deterministic apply plan and dry-run report.
9. Return all stable diagnostics collected during validation.

Dry-run must not:

- mutate the active catalog;
- emit catalog WAL records;
- advance `CatalogVersion`;
- create transaction visibility;
- update plan cache state as truth;
- write audit records required for durable decision visibility;
- use GPU output as validation truth;
- depend on ambient process state, timestamps, random values, or map iteration
  order for accepted results.

### Dry-run report

The dry-run report is bounded validation evidence. It is not database truth.

| Field | Required content |
| --- | --- |
| Batch identity | `DefinitionBatchId`, base version, target version, operation count. |
| Result | Accepted or rejected. |
| Apply order | Deterministic list of operation indexes for accepted batches. |
| Object changes | Created, altered, deprecated, renamed, or updated object version summaries. |
| Dependency evidence | Edges checked, missing edges, cycle diagnostics, compatibility notes. |
| Contract evidence | `ContractHash` values and compatibility classes for Procedure versions. |
| Diagnostics | Stable codes, severity, operation index, source correlation when available. |
| Report hash | Deterministic hash of the bounded report payload for later correlation. |

Rejected dry-run reports must be sufficient for operators and tests to explain
why no mutation was applied.

### Apply behavior

Apply may run only for a dry-run-accepted batch whose base catalog version still
matches the active catalog version or whose implementation has an explicit
serializable retry policy.

Apply sequence:

1. Recheck active base `CatalogVersion`.
2. Reuse or recompute the accepted deterministic apply plan.
3. Revalidate compatibility gates that can change between dry-run and apply.
4. Construct catalog mutation records from the apply plan.
5. Encode catalog WAL records with explicit canonical codecs.
6. Persist catalog WAL records and verify durable WAL coverage.
7. Publish the new active `CatalogVersion` atomically.
8. Emit subscription, audit, and recovery evidence for the accepted publication.
9. Invalidate or update dependent caches using the new `CatalogVersion`,
   `ContractHash`, `StatsVersion`, and policy evidence.

If any apply step fails before publication, the entire batch must remain
invisible. If failure occurs after WAL append but before publication, recovery
must use WAL and publication evidence to decide whether the batch was accepted,
rejected, or stopped at a corruption boundary.

### All-or-nothing behavior

DefinitionBatch is atomic at the catalog publication boundary.

Rules:

1. A rejected dry-run applies nothing.
2. A failed apply publishes nothing unless durable evidence proves a complete
   accepted publication sequence.
3. A partially encoded, partially appended, truncated, or corrupted WAL sequence
   is not a visible batch.
4. A batch must not publish some operations while rejecting others.
5. Recovery must replay a complete accepted batch or reject the batch at a
   documented boundary.
6. Diagnostics and audit evidence must distinguish validation rejection,
   durability failure, publication failure, and recovery rejection.

### WAL and recovery evidence

DefinitionBatch apply must be recoverable from durable catalog WAL evidence.

Required evidence:

| Evidence | Rule |
| --- | --- |
| Batch begin | Identifies batch id, base catalog version, target catalog version, and operation count when the WAL design uses a batch envelope. |
| Operation records | Carry deterministic operation payloads, operation indexes, object identities, object versions, and contract hashes. |
| Batch commit or publication record | Marks the batch as complete and publishable when the WAL design uses an explicit terminal record. |
| LSN range | Records the first and last WAL LSN for the batch. |
| Durable LSN | Must cover the complete accepted batch before visible publication. |
| Payload checksum | Detects corruption or truncation. |
| Replay decision | Recovery report states applied, rejected, skipped, duplicate, or corruption boundary. |
| Catalog version evidence | Replay enforces monotonic target versions. |

DEC-029 lists current catalog WAL record categories. This specification does not
replace the WAL-owned byte format. It defines the DefinitionBatch evidence that
WAL and recovery must preserve.

### Compatibility

DefinitionBatch compatibility decisions must be explicit and versioned.

| Change | Required compatibility evidence |
| --- | --- |
| Create new object | No active conflict and all dependencies visible. |
| Alter Procedure contract | Old hash, new hash, object id, compatibility class, and policy decision. |
| Drop Procedure | Dependency closure, active invocation policy, and deprecation version. |
| Rename or move | Preserved object id, old name, new name, namespace conflict check, dependency update plan. |
| Statistics metadata update | Old and new `StatsVersion`, publication policy, observability and disablement evidence. |
| Policy-linked change | Old and new policy version, permission mapping, and surface admission impact. |

Compatibility acceptance must not rely on implicit null semantics, dynamic
returns, unbounded SRPL semantics, dynamic predicates, or string-built command
text.

### Diagnostics

DefinitionBatch diagnostics must be stable, typed, sanitized, and associated
with the batch and operation index.

| Diagnostic code | Meaning | Required evidence |
| --- | --- | --- |
| `DBATCH-ID-ZERO` | Batch id or operation id is zero. | Field name and operation index. |
| `DBATCH-BASE-VERSION-MISMATCH` | Active catalog version differs from the batch base version. | Expected and actual versions. |
| `DBATCH-TARGET-NON-MONOTONIC` | Target catalog version does not strictly advance. | Base and target versions. |
| `DBATCH-DUPLICATE-NAME` | Two operations produce the same active qualified name. | Namespace, name, operation indexes. |
| `DBATCH-DEPENDENCY-MISSING` | A required dependency is absent or not visible. | Edge kind, selector, version range. |
| `DBATCH-DEPENDENCY-CYCLE` | Dependency graph contains a rejected cycle. | Cycle members and operation indexes. |
| `DBATCH-CONTRACT-HASH-MISMATCH` | Canonical contract bytes do not match supplied hash. | Object id, expected hash, actual hash. |
| `DBATCH-COMPATIBILITY-REJECTED` | Compatibility policy rejected the change. | Object id, compatibility class, reason code. |
| `DBATCH-WAL-NOT-DURABLE` | Apply attempted publication before durable WAL coverage. | Required LSN and observed durable LSN. |
| `DBATCH-RECOVERY-REJECTED` | Recovery rejected the batch sequence. | LSN range, batch id, reason code. |

Diagnostics may include bounded source spans for SRPL-derived operations. They
must not include raw secrets, unbounded source text, raw credentials, private
keys, or host-specific absolute paths unless the path is part of an explicit
operator-selected input.

## Validation

Documentation acceptance checks:

- The spec states that DefinitionBatch is all-or-nothing.
- The spec states that dry-run has no side effects and emits no catalog WAL.
- The spec defines dependency ordering and deterministic tie-breaking.
- The spec states that visible publication requires durable WAL evidence.
- The spec explains recovery behavior for partial, truncated, corrupted, or
  incomplete batch sequences.
- The spec includes compatibility and diagnostics requirements.
- The spec states that DefinitionBatch crate extraction is still pending.
- The spec does not introduce ad hoc SQL, gRPC, runtime JSON defaults, native
  Rust struct layout serialization, or GPU work in critical paths.

Future implementation work should add or keep targeted validation for:

```powershell
cargo test -p andromeda-catalog --test definition_batch_dry_run_contract
cargo test -p andromeda-catalog --test definition_batch_dependency_order
cargo test -p andromeda-catalog --test definition_batch_all_or_nothing
cargo test -p andromeda-storage --test catalog_wal_contract
cargo test -p andromeda-storage --test catalog_batch_recovery_replay
```

These commands are not required for WR12-D documentation-only acceptance.

### Validation matrix

| Area | Acceptance criterion | Evidence |
| --- | --- | --- |
| Batch identity | Batch id, base version, target version, and operation indexes are valid and stable. | Constructor and malformed batch tests. |
| Dry-run purity | Dry-run does not mutate catalog state, emit WAL, or advance `CatalogVersion`. | Before/after state tests and side-effect probes. |
| Dependency ordering | Apply plan is topologically sorted with deterministic tie-breaking. | Dependency graph tests and golden dry-run reports. |
| Conflict detection | Duplicate names, incompatible operations, missing dependencies, and cycles are rejected. | Conflict matrix tests. |
| Contract hashes | Procedure contracts hash deterministically and mismatches are rejected. | Canonical hash and diagnostic tests. |
| Compatibility | Alters, drops, renames, moves, policy changes, and statistics updates require explicit evidence. | Compatibility policy tests. |
| All-or-nothing apply | No partial batch is visible after apply failure. | Failure injection and publication boundary tests. |
| WAL durability | Visible publication waits for durable WAL coverage of the complete accepted batch. | WAL fence and durable LSN tests. |
| Recovery | Replay applies complete accepted batches and rejects incomplete or corrupted sequences. | Recovery replay and crash-injection tests. |
| Diagnostics | Failures produce stable sanitized codes with operation indexes. | Diagnostic snapshot tests. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Dry-run changes active catalog state. | Validation mutated the active store or cache truth. | Treat dry-run output as evidence only and restore mutation-free validation. |
| Apply order changes between runs. | Tie-breaking depends on map iteration, scheduling, or noncanonical values. | Sort by stable operation index and durable identity fields. |
| Some operations publish while others fail. | Apply bypassed all-or-nothing publication. | Reject partial visibility and require complete batch durability evidence. |
| Recovery applies a truncated batch. | Replay accepted incomplete WAL evidence. | Stop at the corruption boundary and preserve a recovery report. |
| A Procedure alter keeps the same contract hash after shape change. | Canonical hash input omitted a contract-affecting field. | Reject with hash mismatch and update canonical contract coverage. |
| Documentation claims a dedicated DefinitionBatch crate exists. | Crate extraction wording drifted ahead of implementation. | Reword to pending extraction and current `andromeda-catalog` ownership. |
| Diagnostics contain raw source or secrets. | Error rendering used unbounded input text. | Replace with bounded spans, stable codes, and sanitized summaries. |

## References

- `documentations/governance/decisions/DEC-029-e4-catalog-wal.md`
- `documentations/governance/decisions/DEC-030-e7-srpl-definitionbatch.md`
- `documentations/specs/CatalogObjectModel_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `crates/README.md`
