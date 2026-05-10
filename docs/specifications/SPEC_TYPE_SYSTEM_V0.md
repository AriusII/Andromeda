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

## V0 type descriptor model

Every contract-visible, persisted, network-visible, binder-visible, and Semantic IR-visible value type must lower to a `TypeDescriptorV0` canonical descriptor. The descriptor is a semantic contract, not a Rust struct layout.

| Field | Requirement |
|---|---|
| `type_kind` | Stable enum tag for `Scalar`, `Domain`, `Enum`, `Optional`, `Collection`, `StructuredObject`, or `Relation`. Unknown tags are rejected. |
| `stable_name` | Canonical qualified name for domain, enum, structured object, or relation types; scalar anonymous descriptors use an empty name. |
| `scalar_policy` | Required for scalar descriptors and must include the policy-specific fields below. |
| `absence_policy` | Required for every value slot; allowed values are `Required` or `ExplicitOptional`. |
| `cardinality` | Required for every expression and relation-valued slot; value slots use `One` or `OptionalOne` only. |
| `field_order` | Required for structured objects and relation rows; ordering is canonical and stable. |
| `element_type` | Required for collection descriptors; collection element absence and collection cardinality are separate descriptors. |
| `domain_base` | Required for domain types and must reference another `TypeDescriptorV0` plus constraint identifiers. |
| `enum_variants` | Required for enum types and must be sorted by stable ordinal, not declaration text position after canonicalization. |

### Scalar policy requirements

| Scalar | Required policy |
|---|---|
| `Int` | Bit width, signedness, overflow policy, and whether the value may participate in identity, key, or permission decisions. |
| `DecimalExact` | Precision, scale, rounding mode, overflow policy, and exactness flag. Precision and scale are mandatory. |
| `Float` | Width and `FloatPolicy`; floats are rejected for keys, durable exact invariants, equality identity, WAL-visible exact values, and permission decisions. |
| `Bool` | Two-state only. A third state requires an enum or explicit optional descriptor. |
| `Text` | `TextPolicy` with UTF encoding, collation, normalization, non-zero maximum length for all contract-visible, persisted, or network-visible shapes, and reject-on-truncate behavior. |
| `Timestamp` | Time scale, timezone interpretation policy, monotonicity policy when used in ordering, and leap-second policy. |

### Absence and optionality

`ExplicitOptional<T>` means the value slot can be absent. It does not authorize a `NULL` sentinel inside `T`, and it does not allow omission of branch handling in SRPL. A relation with `OptionalOne` cardinality means zero or one row; it is not the same as a row containing nullable columns. A structured object field with `ExplicitOptional` means the field value is absent or present with a valid non-null `T`; it does not mean the field can be present with ambient NULL.

Any conversion between `OptionalOne` relation cardinality and `ExplicitOptional<T>` value shape must be explicit in Binder evidence and Semantic IR. The conversion must preserve whether absence means "no row", "field omitted", or "domain-specific absent value". Silent conversion among those meanings is rejected.

### Cardinality model

| Cardinality | Meaning | Required proof |
|---|---|---|
| `Zero` | No value or no rows can be produced. | Static contradiction, impossible branch, or explicit empty relation. |
| `One` | Exactly one value or row. | Unique key, total function, required input, aggregate with defined empty behavior, or runtime cardinality check. |
| `OptionalOne` | Zero or one value or row. | Unique key or runtime cardinality check plus explicit absence branch handling before consumption. |
| `Many` | Zero or more rows. | Explicit read/write-set bound or resource policy bound before execution planning. |
| `BoundedMany` | Zero to N rows. | Non-zero maximum N from contract, predicate proof, catalog metadata, or resource policy. |
| `ExactBounded` | Exactly N rows. | Static shape, fixed tuple/list, or validated runtime count before use. |

Unbounded `Many` is not allowed in a `ProcedureContract`, executable plan, durable evidence object, or network-visible result shape. A runtime-local descriptor may carry `Many` only until Binder either proves a bound or rejects the operation.

### Canonical type descriptor form

Canonical type descriptor bytes are used for contract hashing, Semantic IR hashing, catalog shape hashes, and golden tests. They are not a persisted Rust layout.

| Component | Rule |
|---|---|
| Domain separator | ASCII `TypeDescriptorV0` plus one byte format version. |
| Integer encoding | Little-endian fixed-width integers. |
| Enum tags | Stable one-byte or two-byte tags defined by the implementation and covered by golden tests. |
| Strings | UTF-8 length-prefixed by `u32`, normalized according to `TextPolicy` where applicable. |
| Field lists | Length-prefixed and ordered by canonical field ordinal. |
| Sets | Sorted by stable identifier before encoding. |
| Hash coverage | Covers type kind, scalar policy, absence policy, cardinality, field order, names, enum variants, domain constraints, and collection element descriptors. |
| Exclusions | Excludes source spans, comments, whitespace, Rust type names, memory addresses, map iteration order, and debug formatting. |

## Invariants

- No ambient NULL.
- Absence is represented explicitly through `OptionalPolicy`; ambient NULL is not a type-system state.
- Optional value absence, absent relation row, omitted field, and empty collection are four distinct states and must not be conflated.
- `DecimalExact` is required for money, counters, inventory, identity, and durable exact invariants.
- Boolean is two-state unless an Enum defines more states.
- `FloatPolicy` is controlled and cannot be used for keys, equality identity, WAL-visible exact values, or permission decisions.
- Text declares encoding, collation, normalization, length, and rejection policy through `TextPolicy`.
- Text without an explicit maximum is runtime-local only and must not appear in ProcedureContract, persisted, or network-visible V0 shapes.
- Cardinality is part of expression type.
- Cardinality must be preserved into ProcedureContract shapes and Semantic IR.
- Type descriptor identity is based on canonical descriptor bytes, not raw source text, debug output, serde defaults, or native Rust layout.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.
- `repr(Rust)`, derived serializer field order, pointer identity, discriminant values without stable tags, and platform-dependent alignment are forbidden as type identity or hash inputs.

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

Type-system validation must expose stable rejection codes for at least:

| Code | Condition |
|---|---|
| `TYPE-ABSENCE-AMBIENT-NULL` | Any descriptor, SRPL type, contract shape, or IR value attempts to represent ambient NULL. |
| `TYPE-OPTIONAL-UNCHECKED` | An optional value or optional-one relation is consumed without explicit branch handling. |
| `TYPE-CARDINALITY-UNPROVEN-ONE` | `One` cardinality lacks uniqueness proof or a runtime cardinality check. |
| `TYPE-CARDINALITY-UNBOUNDED` | `Many` reaches a contract, network, persisted, or executable-plan boundary without a bound. |
| `TYPE-TEXT-UNBOUNDED-SURFACE` | Text without a maximum reaches a contract, persisted, or network-visible shape. |
| `TYPE-FLOAT-EXACT-INVARIANT` | Float is used for key, identity, durable exact invariant, WAL-visible exact value, or permission decision. |
| `TYPE-CANONICAL-LAYOUT-RUST` | Native Rust layout, debug text, or unstable serializer output is used as semantic identity. |

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
- optional value vs optional-one relation distinction tests.
- cardinality proof and unbounded-many rejection tests.
- text encoding, normalization, and maximum-length rejection tests.
- enum flags unknown-bit rejection tests.
- canonical TypeDescriptorV0 golden byte/hash tests that are stable across formatting and platforms.

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
- Reject `optional row treated as nullable column`.
- Reject `empty collection treated as absent value`.
- Reject `TypeDescriptor hash derived from Rust native layout`.
- Reject `TypeDescriptor hash derived from debug formatting`.

## Acceptance summary

Owner: TypeSystem and SRPL contract owners must maintain the canonical `TypeDescriptorV0`, absence, scalar policy, text policy, and cardinality rules across type construction, Binder, Semantic IR, ProcedureContract, catalog shape hashes, persisted evidence, and network-visible shapes.

Evidence: Acceptance requires golden byte/hash tests for `TypeDescriptorV0`, decimal/float/text policy tests, optional value versus optional-one relation tests, cardinality proof tests, unbounded-many rejection tests, and formatting/platform stability evidence showing semantic identity does not depend on source text, debug output, or native Rust layout.

Reject: Acceptance requires fail-closed proof for ambient NULL, implicit nullable fields, unchecked optional consumption, unbounded contract-visible text, unbounded many at contract or execution boundaries, float use in exact/key/permission decisions, silent numeric conversion, and any TypeDescriptor identity derived from Rust native layout.
