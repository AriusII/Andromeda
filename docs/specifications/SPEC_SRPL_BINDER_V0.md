# Specification: SRPL Binder v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `SRPL Binder v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Resolve names, types, cardinalities, effects, permissions, and read/write sets.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `BindingContext` | Must be represented as an explicit typed structure or canonical descriptor. |
| `NameResolution` | Must be represented as an explicit typed structure or canonical descriptor. |
| `TypeBinding` | Must be represented as an explicit typed structure or canonical descriptor. |
| `CardinalityBinding` | Must be represented as an explicit typed structure or canonical descriptor. |
| `EffectBinding` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ReadWriteSet` | Must be represented as an explicit typed structure or canonical descriptor. |
| `AbsenceBinding` | Explicit optional/absence handling required before lowering. |
| `StableDiagnostic` | Binder diagnostic with stable code, span, and referenced symbol when applicable. |

## V0 binding model

The Binder consumes parsed SRPL plus a catalog snapshot and produces canonical binding evidence for Semantic IR and `ProcedureContract` construction. Binding evidence is a semantic descriptor; it must not depend on Rust memory layout, map iteration order, source whitespace, comments, or diagnostic message wording.

| Descriptor | Required fields |
|---|---|
| `BindingContext` | `CatalogVersion`, active namespace, procedure name, source identity reference, policy version when available, and closed import/scope list. |
| `NameResolution` | Source symbol id, source span, canonical catalog object id, object kind, stable object name, catalog version, and ambiguity status. |
| `TypeBinding` | Source expression id, `TypeDescriptorV0`, absence policy, conversion rule if any, and rejection code on failure. |
| `CardinalityBinding` | Source expression id, cardinality, proof kind, proof evidence reference, runtime check requirement, and max row bound when cardinality can produce many rows. |
| `EffectBinding` | Operation ordinal, effect kind, read set reference, write set reference, required permission ids, transaction requirement, and WAL requirement. |
| `ReadWriteSet` | Ordered read objects, ordered write objects, object shape hashes, access modes, row-bound evidence, predicate-bound evidence, and catalog version. |
| `AbsenceBinding` | Optional source id, branch construct id, present path type, absent path behavior, and evidence that no unchecked optional consumption remains. |
| `StableDiagnostic` | Stable code, phase, source span, symbol/object reference when applicable, context keys, and non-contractual human message. |

### Name resolution

Every identifier that can affect execution must resolve to exactly one canonical target before lowering. The target must be represented by stable catalog identity, not by display name alone. Resolution must reject:

- names that bind to more than one visible catalog object;
- names that bind to no catalog object when a catalog object is required;
- dynamic table, field, permission, result stream, or procedure names;
- shadowing that changes the target of an already-bound identifier in the same procedure;
- names whose canonical identity changes between `CatalogVersion` values without a new binding pass.

### Type, absence, and cardinality binding

Binder must preserve the distinction between value absence and relation cardinality:

| Source situation | Required binder result |
|---|---|
| Required value input | `TypeBinding.absence_policy = Required`, `CardinalityBinding = One`. |
| Explicit optional value input | `TypeBinding.absence_policy = ExplicitOptional`, branch handling required before field/value consumption. |
| Read returning `OptionalOne` | Relation cardinality is `OptionalOne`; row absence must be handled before row fields are consumed. |
| Read returning `One` | Requires unique key proof, catalog constraint proof, or runtime cardinality check before execution can observe the value. |
| Read returning `Many` | Requires an explicit row bound or resource-policy bound before executable planning. |
| Empty result path | Must carry `Zero` cardinality or an explicit absent branch, never ambient NULL. |

Unchecked optional consumption must be rejected before Semantic IR lowering. Lowering is allowed to contain an explicit branch marker for absence, but not an unresolved optional dereference.

### Read/write set binding

Read and write sets are contract inputs. They must be complete before `ProcedureContract` hashing and before executable planning.

| Field | Requirement |
|---|---|
| `read_objects` | Canonical object ids read directly or through predicates, sorted by stable id after operation-order evidence is recorded. |
| `write_objects` | Canonical object ids mutated directly or through emitted durable side effects, sorted by stable id after operation-order evidence is recorded. |
| `access_mode` | `Read`, `Insert`, `Update`, `Delete`, `Emit`, or closed V0 effect tag. |
| `shape_hash` | Non-zero canonical hash of the bound object shape at the active `CatalogVersion`. |
| `row_bound` | Required for any object access that can produce or affect multiple rows. |
| `predicate_bound` | Required evidence that predicates are closed typed expressions, not dynamic text. |
| `permission_ids` | Stable permission ids required for each access mode. |
| `wal_coverage` | Required for every write object before mutation is admitted. |

Mutation binding must fail closed unless the object is WAL-covered, the required write permission appears in the contract candidate, and the read/write set is bounded.

### Stable diagnostics

Diagnostic messages are explanatory text and are not the compatibility contract. Tests, IDE integrations, audit evidence, and DefinitionBatch reports must key on stable diagnostic codes plus phase and span.

| Code family | Required use |
|---|---|
| `SRPL-BIND-NAME-*` | Missing, ambiguous, dynamic, or shadowed name. |
| `SRPL-BIND-TYPE-*` | Type mismatch, forbidden conversion, forbidden ambient NULL, or unsupported type kind. |
| `SRPL-BIND-CARD-*` | Missing uniqueness proof, row-count check failure, or unbounded many. |
| `SRPL-BIND-ABSENCE-*` | Unchecked optional, erased absence, or invalid optional branch. |
| `SRPL-BIND-EFFECT-*` | Missing permission, missing WAL coverage, unbounded read/write set, or dynamic effect. |
| `SRPL-BIND-CONTRACT-*` | Missing contract field, incompatible shape, or unstable hash input. |

Adding a new code is additive only when existing codes keep their meaning. Reusing a code for a different condition is breaking.

## Invariants

- Every name resolves to one catalog object.
- Absence is explicit; optional one requires branch handling.
- Cardinality `one` requires uniqueness proof or runtime cardinality check.
- Mutation requires WAL-covered object.
- Read/write sets are contract inputs and cannot be inferred later by execution helpers.
- Binder diagnostics are stable across formatting-only source changes.
- Binder output is deterministic for the same source semantics, catalog version, and policy version.
- Binder output must not contain source SQL text, dynamic object names, unchecked optional dereferences, unbounded multi-row effects, or unresolved permissions.
- `ReadWriteSet` is hashed into `ProcedureContract` effects and referenced by Semantic IR effect boundaries.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.
- Binder evidence used for contract hashing, audit, or durable DefinitionBatch reports must use canonical descriptors with stable tags, length prefixes, and sorted sets.
- Native Rust layout, derived serializer field order, debug output, pointer identity, and hash-map iteration order must not define binder identity.

### Canonical binder evidence form

Binder evidence canonicalization uses:

| Component | Rule |
|---|---|
| Domain separator | ASCII `SrplBinderEvidenceV0` plus one byte format version. |
| Context | `CatalogVersion`, `PolicyVersion` when applicable, and canonical procedure name. |
| Resolutions | Sorted by source symbol id, each carrying canonical object id and object kind. |
| Types | Sorted by expression id, each carrying canonical `TypeDescriptorV0` bytes by reference or digest. |
| Cardinalities | Sorted by expression id, carrying cardinality tag, proof kind, and bound. |
| Effects | Sorted by operation ordinal, carrying read/write set digests and required permission ids. |
| Diagnostics | Rejected bindings carry stable code, phase, span, and context keys; diagnostic human text is excluded from semantic hash. |

## State transitions

State transitions must be explicit. Invalid transitions return typed errors and emit trace evidence when they affect execution, storage, security, or recovery.

## Error model

| Error family | Use |
|---|---|
| ContractError | Invalid shape, incompatible hash, missing contract field. |
| PermissionError | Principal lacks required permission or surface scope. |
| ResourceError | Budget, quota, backpressure, or timeout failure. |
| TransactionError | Isolation, rollback, commit, or serialization failure. |
| StorageError | WAL, page, segment, manifest, or corruption failure. |
| SystemError | Internal condition requiring poison, rollback, forensic, or restore path. |

Binder-specific failures must map to stable diagnostic codes first and then to the appropriate error family for publication, admission, or execution reporting.

## Security model

Security-sensitive operations require admission through identity, principal, permission, policy, and audit checks before durable mutation or transaction creation.

## Observability

At minimum, implementations must emit trace evidence with:

```text
TraceId
InvocationId when applicable
CatalogVersion when applicable
PolicyVersion when applicable
Result
ErrorKind when applicable
```

## Recovery behavior

If this specification affects durable state, it must define how recovery replays, validates, rebuilds, or rejects the affected state.

## Compatibility

Changes are classified as:

| Change | Default status |
|---|---|
| Add optional field with explicit default | Additive |
| Add required field | Breaking |
| Change type or cardinality | Breaking |
| Change security requirement | Security-impact |
| Change recovery behavior | Breaking unless explicitly versioned |

## Tests

- name ambiguity tests.
- cardinality mismatch tests.
- read/write inference tests.
- optional branch tests.
- stable binder diagnostic tests.
- read/write set canonicalization tests.
- mutation permission and WAL-covered object rejection tests.
- optional-one unchecked consumption rejection tests.
- formatting-insensitive binder evidence hash tests.

## Rejection criteria

- Reject `ambiguous name`.
- Reject `unchecked optional`.
- Reject `absence erased before IR`.
- Reject `mutation without write permission`.
- Reject `unbounded read/write set`.
- Reject `unstable binder diagnostic code`.
- Reject `write object without WAL coverage`.
- Reject `ReadWriteSet inferred after contract hashing`.
- Reject `diagnostic keyed only by message text`.
- Reject `binder evidence hash derived from Rust native layout`.
- Reject `dynamic object name`.
- Reject `source SQL text in bound predicate`.

## Acceptance summary

Owner: SRPL Binder owners must maintain name resolution, type binding, absence binding, cardinality binding, effect binding, read/write-set binding, stable diagnostics, and canonical binder evidence before Semantic IR lowering or ProcedureContract hashing.

Evidence: Acceptance requires tests and retained evidence for unambiguous catalog resolution, stable diagnostic codes, formatting-insensitive binder evidence hashes, complete `ReadWriteSet` canonicalization, optional-one branch handling, cardinality proof or runtime-check markers, mutation permission validation, WAL-covered write validation, and contract hash inputs that include read/write effects before execution planning.

Reject: Acceptance requires fail-closed proof for ambiguous or dynamic names, unchecked optional consumption, absence erased before IR, unbounded read/write sets, mutation without write permission, write targets without WAL coverage, source SQL text in bound predicates, diagnostics keyed only by message text, and binder evidence identity derived from native Rust layout.
