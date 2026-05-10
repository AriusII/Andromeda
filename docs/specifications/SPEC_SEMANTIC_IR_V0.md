# Specification: Semantic IR v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `Semantic IR v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define the stable semantic representation produced from SRPL.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `IrProcedure` | Must be represented as an explicit typed structure or canonical descriptor. |
| `IrRelationExpr` | Must be represented as an explicit typed structure or canonical descriptor. |
| `IrMutation` | Must be represented as an explicit typed structure or canonical descriptor. |
| `IrPredicate` | Must be represented as an explicit typed structure or canonical descriptor. |
| `IrAggregate` | Must be represented as an explicit typed structure or canonical descriptor. |
| `IrEffect` | Must be represented as an explicit typed structure or canonical descriptor. |
| `IrContractBinding` | Must be represented as an explicit typed structure or canonical descriptor. |
| `IrCardinality` | Cardinality carried after binding and before execution planning. |
| `IrAbsence` | Explicit absence branch or rejection marker. |
| `IrDiagnosticRef` | Stable diagnostic reference retained for rejected lowering decisions. |

## V0 Semantic IR model

Semantic IR is the formatting-insensitive typed representation between Binder and planning. It is not source text, not AST, not a `ProcedureContract`, not an executable plan, and not a persisted Rust layout.

| IR descriptor | Required fields |
|---|---|
| `IrProcedure` | Canonical procedure name, `CatalogVersion`, `SemanticIrVersion`, input bindings, result stream bindings, operation list, effect summary, and contract binding reference. |
| `IrRelationExpr` | Relation operator tag, input relation references, predicate ids, projection ids, set semantics tag, cardinality, row-bound evidence, and ordering policy. |
| `IrMutation` | Target object id, operation tag, input relation/value refs, write-set ref, required permissions, WAL coverage evidence, and transaction/effect boundary. |
| `IrPredicate` | Closed typed predicate enum, input refs, comparison/operator tag, type descriptor refs, and no raw executable text. |
| `IrAggregate` | Aggregate tag, input relation ref, grouping keys, empty-case policy, output type, cardinality, and deterministic ordering policy when needed. |
| `IrEffect` | Effect id, operation ordinal range, read-set digest, write-set digest, required permissions, isolation requirement, resource bound, and audit evidence requirement. |
| `IrContractBinding` | Procedure name, `ContractHash`, contract shape digest, read/write-set digest, protocol layout ref, and compatibility evidence. |
| `IrCardinality` | Cardinality tag, proof kind, max row bound when applicable, and runtime check marker when needed. |
| `IrAbsence` | Explicit present/absent branch, absent behavior, source optional binding ref, and rejection marker for unchecked optional use. |
| `IrDiagnosticRef` | Stable diagnostic code, phase, span, and context keys; human message is excluded from semantic identity. |

### Semantic boundaries

| Surface | Durable? | Network-visible? | Canonical identity | Recovery behavior |
|---|---:|---:|---|---|
| SRPL source text | DefinitionBatch evidence only | No | Source digest, not semantic identity | Re-parse or reject if digest mismatch. |
| AST | No | No | None; compiler-local only | Rebuild from source. |
| Binder evidence | DefinitionBatch evidence when published | No | Canonical binder evidence digest | Rebind from source/catalog or reject on catalog drift. |
| Semantic IR | No by default; durable only if separately versioned | No | `SemanticIrHash` | Rebuild from Binder evidence or reject if stored version unsupported. |
| ProcedureContract | Yes, catalog-visible | Yes by reference/hash | `ContractHash` | Validate against catalog and compatibility policy. |
| Executable plan | Plan-cache only unless separately specified | No | Plan-cache key, not `SemanticIrHash` | Rebuild after version, policy, stats, or catalog drift. |

Semantic IR may be cached, but cache entries are invalid unless their `SemanticIrHash`, `ContractHash`, `CatalogVersion`, `PolicyVersion`, and relevant `StatsVersion` bindings still match the cache policy.

### Set, ordering, and aggregate semantics

All relation expressions must carry a closed collection semantics tag. V0 relation semantics are `Set` unless a future version explicitly adds another tag. Implicit bag semantics are rejected. Ordering is absent unless an IR node carries an explicit ordering policy; source text order must not imply execution order except for operation ordinals and effect boundaries.

Aggregates must define their empty-input behavior in IR. An aggregate that can return no row carries `OptionalOne`; an aggregate that returns a default value on empty input carries `One` plus the default policy. The empty-case decision is part of `SemanticIrHash`.

### Absence in IR

IR must preserve optional value absence and optional-one row absence as distinct forms. Field access, predicate evaluation, mutation, emit, or aggregate consumption of an optional value or optional-one row must be dominated by an explicit `IrAbsence` present branch or rejected with a stable diagnostic. IR must not encode absence as a null literal, sentinel scalar, missing map key, or shape-changing row.

### Semantic hash form

`SemanticIrHash` is distinct from `ContractHash`. `ContractHash` identifies the callable contract shape and policy. `SemanticIrHash` identifies the canonical semantics of the bound procedure body and bindings that feed planning.

| Canonical input group | Required content |
|---|---|
| Domain separator | ASCII `SemanticIRV0` plus one byte format version. |
| Version | `SemanticIrVersion` and canonical codec version. |
| Procedure identity | Canonical procedure name and object kind; volatile `ProcedureId` is binding evidence, not semantic body identity. |
| Bindings | Digests or inline canonical forms for name, type, cardinality, absence, and read/write-set bindings. |
| Operations | Operation ordinals, closed operation tags, input refs, output refs, relation semantics, mutation/effect tags, and branch structure. |
| Types | `TypeDescriptorV0` digests or inline canonical descriptors. |
| Effects | `IrEffect` descriptors, permission ids, isolation/resource policy refs, WAL coverage evidence, and audit requirements. |
| Diagnostics | Stable diagnostic refs for rejected lowering decisions; human messages excluded. |
| Exclusions | Raw source text, comments, whitespace, spans except diagnostic refs, Rust type names, debug formatting, memory addresses, and map iteration order. |

Formatting-only SRPL changes must not change `SemanticIrHash`. Catalog, permission, type, cardinality, absence, read/write-set, or effect changes that alter semantics must change `SemanticIrHash`.

### Canonical IR encoding

Canonical IR bytes are used for `SemanticIrHash` and golden vectors. They are not a native Rust layout and are not automatically a persisted storage format.

| Component | Rule |
|---|---|
| Integer encoding | Little-endian fixed-width integers. |
| Tags | Stable closed tags for operations, predicates, values, cardinalities, absence branches, relation semantics, effects, and diagnostics. |
| Strings | UTF-8 `u32` length-prefixed canonical names. |
| Lists | Length-prefixed and ordered by semantic ordinal. |
| Sets | Sorted by canonical stable identifier before encoding. |
| References | Use canonical ids or canonical digests; never pointers or allocation indexes. |
| Unknown fields | Rejected in V0 hash input unless an explicit future-version skip rule is defined. |
| Hash algorithm | Cryptographic hash selected by implementation policy and locked by golden vectors. |

## Invariants

- IR is independent from formatting.
- IR hash is deterministic.
- IR represents set semantics explicitly.
- IR carries cardinality explicitly.
- IR carries absence explicitly.
- IR records effect boundaries.
- IR is not a persisted storage format unless separately versioned and encoded.
- `SemanticIrHash` is distinct from `ContractHash`.
- IR contains closed typed operations and predicates, not dynamic SQL, source SQL text, or runtime expression strings.
- IR read/write sets, required permissions, cardinality proofs, and WAL coverage evidence are inherited from Binder and cannot be invented by execution helpers.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.
- Native Rust layout must not be used as Semantic IR identity or exported evidence.
- `Debug`, serde default order, enum discriminants without stable tags, pointer identity, allocation order, and platform alignment must not define IR identity or hash input.

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

Semantic IR failures must expose stable diagnostic codes for at least:

| Code | Condition |
|---|---|
| `SRPL-IR-HASH-SOURCE-TEXT` | Raw source text or formatting-sensitive input is used as semantic identity. |
| `SRPL-IR-CARDINALITY-DROPPED` | Cardinality is absent from a lowered node that requires it. |
| `SRPL-IR-ABSENCE-DROPPED` | Optional value or optional-one row absence is erased during lowering. |
| `SRPL-IR-EFFECT-MISSING` | Operation lacks required effect boundary, permission, or read/write-set reference. |
| `SRPL-IR-SET-IMPLICIT-BAG` | Relation semantics are omitted or imply bag behavior. |
| `SRPL-IR-NATIVE-LAYOUT` | Native Rust layout, debug output, or unstable serializer order is used as identity. |
| `SRPL-IR-DYNAMIC-TEXT` | Forbidden executable text, dynamic SQL, or raw predicate string reaches IR. |

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

- source formatting stability tests.
- IR hash golden tests.
- aggregate empty-case tests.
- effect serialization tests.
- absence/cardinality preservation tests.
- set semantics rejection tests for implicit bag behavior.
- canonical Semantic IR byte golden tests.
- `SemanticIrHash` distinct-from-`ContractHash` tests.
- optional-one branch dominance tests.
- read/write-set and effect-boundary preservation tests from Binder to IR.

## Rejection criteria

- Reject `raw source hash as semantic identity`.
- Reject `implicit bag semantics`.
- Reject `missing effect annotation`.
- Reject `cardinality dropped during lowering`.
- Reject `absence dropped during lowering`.
- Reject `SemanticIrHash equals raw source hash`.
- Reject `SemanticIrHash substituted with ContractHash`.
- Reject `IR hash derived from Rust native layout`.
- Reject `raw predicate string in IR`.
- Reject `unchecked optional in IR`.
- Reject `write effect without WAL coverage evidence`.

## Acceptance summary

Owner: Semantic IR owners must maintain `SemanticIrVersion`, canonical IR descriptors, `SemanticIrHash`, explicit set semantics, cardinality, absence, read/write-set references, effect boundaries, and contract binding references between Binder and planning.

Evidence: Acceptance requires canonical Semantic IR byte golden tests, formatting-insensitive `SemanticIrHash` tests, proof that `SemanticIrHash` is distinct from `ContractHash`, absence/cardinality preservation tests, optional-one branch dominance tests, set semantics tests, read/write-set and effect-boundary preservation tests, and platform-stability evidence excluding native Rust layout from identity.

Reject: Acceptance requires fail-closed proof for raw source hash as semantic identity, `ContractHash` substituted for `SemanticIrHash`, implicit bag semantics, missing effect annotations, dropped cardinality, dropped absence, unchecked optional use, raw predicate strings or dynamic SQL text in IR, write effects without WAL coverage evidence, and any IR hash derived from Rust native layout.
