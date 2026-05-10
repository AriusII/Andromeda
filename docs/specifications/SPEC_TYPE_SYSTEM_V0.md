# Specification: TypeSystem v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `TypeSystem v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define primitive, domain, enum, optional, collection, relation, and StructuredObject types.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `ScalarType` | Must be represented as an explicit typed structure or canonical descriptor. |
| `DomainType` | Must be represented as an explicit typed structure or canonical descriptor. |
| `EnumType` | Must be represented as an explicit typed structure or canonical descriptor. |
| `OptionalType` | Must be represented as an explicit typed structure or canonical descriptor. |
| `CollectionType` | Must be represented as an explicit typed structure or canonical descriptor. |
| `StructuredObjectType` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RelationType` | Must be represented as an explicit typed structure or canonical descriptor. |
| `DecimalExact` | Exact decimal type with explicit precision, scale, rounding, and overflow policy. |
| `FloatPolicy` | Controlled floating-point policy for non-key, non-durable-exact use. |
| `TextPolicy` | Encoding, collation, normalization, maximum length, and truncation rejection policy. |
| `OptionalPolicy` | Explicit absence policy; absence is not ambient NULL. |
| `Cardinality` | Zero, one, optional-one, many, bounded-many, or exact bounded relation cardinality. |

## Invariants

- No ambient NULL.
- Absence is represented explicitly through `OptionalPolicy`; ambient NULL is not a type-system state.
- `DecimalExact` is required for money, counters, inventory, identity, and durable exact invariants.
- Boolean is two-state unless an Enum defines more states.
- `FloatPolicy` is controlled and cannot be used for keys, equality identity, WAL-visible exact values, or permission decisions.
- Text declares encoding, collation, normalization, length, and rejection policy through `TextPolicy`.
- Text without an explicit maximum is runtime-local only and must not appear in ProcedureContract, persisted, or network-visible V0 shapes.
- Cardinality is part of expression type.
- Cardinality must be preserved into ProcedureContract shapes and Semantic IR.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

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

- type constructor tests.
- decimal exact precision, scale, rounding, and overflow tests.
- float-as-key rejection tests.
- optional branch requirement tests.
- text encoding, normalization, and maximum-length rejection tests.
- enum flags unknown-bit rejection tests.

## Rejection criteria

- Reject `implicit nullable field`.
- Reject `ambient NULL semantics`.
- Reject `decimal without precision and scale`.
- Reject `silent numeric conversion`.
- Reject `float key`.
- Reject `float permission decision`.
- Reject `unbounded text`.
- Reject `unbounded text in ProcedureContract`.
- Reject `unbounded text in persisted or network-visible shape`.
- Reject `cardinality erased before contract hashing`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
