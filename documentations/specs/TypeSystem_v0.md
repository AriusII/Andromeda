# TypeSystem v0 Specification

## Purpose

Define the accepted documentation contract for `TypeSystem v0`, the foundation
type vocabulary used by Procedure contracts, SRPL binding, catalog object
definitions, ResultStream shapes, and StructuredObject metadata.

`TypeSystem v0` makes value shape, absence, identity, compatibility, and
validation explicit before execution. It is a contract boundary. It is not a
SQL compatibility layer, not an implicit-null model, and not a claim that every
future scalar or domain type is already implemented.

## Scope

This specification applies to the R0 and R1 type contracts currently centered
on:

- `andromeda-types` for scalar descriptors, absence policy, column descriptors,
  shared IDs, catalog versions, and `ContractHash`;
- `andromeda-contract` for Procedure contract type use, ResultStream
  cardinality, binding evidence, `StatsVersion`, and `PolicyVersion`;
- `andromeda-structured-object` for StructuredObject headers, layouts,
  descriptor hashes, row-count metadata policy, and metadata-before-payload
  validation;
- SRPL binder and catalog consumers that must reject incompatible, ambiguous,
  or unbounded type shapes before transaction creation.

It covers:

- scalar families accepted in v0;
- absence and nullability semantics;
- decimal, float, and text rules;
- ID, version, and hash newtypes;
- StructuredObject ties and descriptor hashing;
- Procedure and catalog compatibility rules;
- validation gates and pending ownership gaps.

## Current Implementation Status

`andromeda-types` currently models `TypeDescriptor` as `ScalarType` plus
`AbsencePolicy`. The supported scalar model includes signed and unsigned
integers, decimals, floats, booleans, text, and timestamps. `ColumnDescriptor`
adds a field name and ordinal and validates per-column shape.

`andromeda-structured-object` consumes `ColumnDescriptor` values and includes
field names, ordinals, scalar descriptors, absence policy, and physical layout
in the deterministic descriptor hash. The descriptor hash is independent of the
StructuredObject name.

`andromeda-contract` consumes the same column descriptors for Procedure inputs
and ResultStream columns. Procedure binding evidence includes `ProcedureId`,
`CatalogVersion`, `ContractHash`, `StatsVersion`, and `PolicyVersion`.

The current implementation does not fully settle every ownership boundary.
Pending gaps are listed in the "Pending gaps" section and must not be treated as
silent approvals.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL, dynamic table names, dynamic
  predicates, shape-shifting returns, or generic query text;
- authorize implicit null semantics or SQL-style three-valued predicate logic;
- serialize Rust native structs directly to disk or network;
- define persistent page, WAL, manifest, or network byte layouts;
- move `StatsVersion`, `PolicyVersion`, `Lsn`, `PageId`, or storage identifiers
  between crates;
- define a full decimal arithmetic engine, locale collation engine, Unicode
  normalization engine, or floating-point reproducibility runtime;
- define JSON as a runtime payload format. JSON projections are diagnostic and
  observability-only unless another spec explicitly narrows that use;
- add GPU, analytics, benchmark, or learned-output authority to type
  validation, commit, WAL, recovery, catalog publication, or security-critical
  paths.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for Andromeda non-negotiable invariants.
- `docs/adr/ADR-0011-workspace-crate-boundaries.md` for R0 and R1 crate
  ownership.
- `crates/README.md` for workspace boundary rules.
- `crates/andromeda-types/src/types.rs` for current scalar and absence code
  evidence.
- `crates/andromeda-types/src/ids.rs` for current ID and `ContractHash`
  evidence.
- `crates/andromeda-contract/src/contracts/types.rs` for Procedure,
  ResultStream, `StatsVersion`, and `PolicyVersion` evidence.
- `crates/andromeda-structured-object/src/lib.rs` and
  `crates/andromeda-structured-object/src/hash.rs` for StructuredObject
  metadata and descriptor-hash evidence.
- `documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md` for consolidated
  type-system direction.
- `documentations/reference/json-schema.md` only for diagnostic JSON projection
  vocabulary, not as normative runtime wire authority.

## Procedure

### Ownership

`andromeda-types` owns contract-safe foundation types. It may define stable
IDs, versions, scalar descriptors, column descriptors, and contract hashes. It
must not depend on catalog stores, execution, storage runtime, WAL runtime,
QUIC, Protobuf runtime projection, benchmark, analytics, GPU, SQL crates, or
implicit native-layout serialization dependencies.

`andromeda-contract` owns Procedure contracts, ResultStream contracts, catalog
object descriptors, structural dependency edges, and binding evidence. It may
consume foundation type descriptors and must validate them before Procedure
contract publication.

`andromeda-structured-object` owns StructuredObject metadata, not payload
serialization. It validates headers before payload bytes are admitted and must
not depend on `andromeda-contract`, catalog stores, SRPL runtime, execution,
storage, QUIC, Protobuf runtime, benchmark, analytics, or GPU crates.

### Type descriptor model

Every contract-visible value shape must be represented as:

```text
TypeDescriptor = ScalarType + AbsencePolicy
ColumnDescriptor = name + TypeDescriptor + ordinal
```

`ScalarType` describes the value domain. `AbsencePolicy` describes whether the
value may be absent. `ColumnDescriptor` adds stable, named, zero-based field
position evidence for tables, Procedure inputs, ResultStream columns, and
StructuredObject fields.

Column collections must validate cross-column rules outside the individual
column:

- names are non-empty;
- names are unique within the collection;
- ordinals are dense and zero-based;
- each field validates its scalar and absence descriptor;
- compatibility decisions use names and ordinals deliberately, never physical
  payload position alone.

### Scalar families

`TypeSystem v0` accepts these scalar families:

| Family | Current shape | Rule |
| --- | --- | --- |
| Signed integer | `I8`, `I16`, `I32`, `I64`, `I128` | Width is explicit. Silent widening, narrowing, and signed/unsigned conversion are not allowed. |
| Unsigned integer | `U8`, `U16`, `U32`, `U64`, `U128` | Width is explicit. Signed interop requires an explicit checked conversion rule. |
| Decimal | `Min`, `Mid`, `Max`, `Custom { precision, scale }` | Exact numeric family. Custom precision must be positive and scale must not exceed precision. |
| Float | `Min`, `Mid`, `Max`, `Custom { bits, mode }` | Approximate or analytics family. Custom bits must be 16, 32, 64, or 128 and current validation requires deterministic analytics mode. |
| Bool | `Bool` | Two-state boolean. Must not introduce implicit business absence or SQL-style three-valued predicate logic. |
| Text | `Text { encoding, max_length, collation }` | Encoding is explicit. `max_length`, when present, must be positive. Collation is explicit when comparison semantics matter. |
| Timestamp | `Transaction`, `Invocation`, `MonotonicEpoch` | Time derivation is explicit. Raw ambient wall-clock dependence is not a contract-stable value source. |

Future scalar additions, including binary or domain-specific wrappers, require a
compatibility review and tests that prove hash stability, validation behavior,
and projection behavior.

### Absence and nullability

`TypeSystem v0` has two absence policies:

| Policy | Meaning | Runtime implication |
| --- | --- | --- |
| `Required` | A value must be present. | Missing input, missing field, or absent payload value is a contract failure. |
| `ExplicitOptional` | Absence is explicitly permitted. | Callers and SRPL must handle the absent branch explicitly. |

Absence is not an ambient `NULL`. It is part of the contract shape and
participates in contract and descriptor hashes. A value that is absent is not a
third truth value inside predicates. Predicate cores remain boolean; optional
values must be pattern-matched, branched, or rejected before comparison.

Diagnostic JSON may expose `nullable: true` or JSON `null` for observability.
That projection is not normative runtime semantics. Runtime contracts must map
diagnostic nullability back to `AbsencePolicy` and reject unsupported implicit
null behavior.

### Decimal rules

Decimals are the exact numeric family. Use decimals for money, accounting,
quantities that must balance, tax values, contractual rates, and values that
participate in exact relational invariants.

Validation rules:

- `Custom { precision, scale }` requires `precision > 0`.
- `scale <= precision`.
- A decimal type used across a Procedure boundary must participate in the
  Procedure `ContractHash`.
- Decimal widening, scale change, or precision change is compatibility-relevant
  and must not be silently accepted.

### Float rules

Floats are controlled approximations. Use floats for measurements, scores,
statistics, analytics, vector features, and advisory evidence where exact
relational truth is not required.

Validation rules:

- custom float widths are limited to 16, 32, 64, or 128 bits;
- current custom float validation requires `DeterministicAnalytics`;
- floats must not back primary keys, foreign keys, exact uniqueness,
  accounting, consensus, commit visibility, or other exact transactional
  invariants;
- floats must not decide catalog publication, WAL durability, recovery truth,
  IAM authorization, or commit outcome;
- any approximate float behavior must be observable and explainable when used
  by optimizer, statistics, or analytics components.

### Text rules

Text values must carry explicit encoding. Current accepted encodings are
`Utf8`, `Utf16`, and `Unicode`.

Validation rules:

- `max_length = Some(0)` is invalid;
- unbounded text must be justified by the owning contract and still respect RPC,
  resource, and payload bounds;
- collation must be declared when equality, ordering, uniqueness, or indexing
  semantics depend on comparison policy;
- normalization requirements must be owned by the contract or catalog rule that
  needs them. They are not implicit in `TextType`.

### ID, version, and hash newtypes

Andromeda uses semantic newtypes rather than primitive aliases for identifiers
and versions.

Current `andromeda-types` ID wrappers preserve the supplied `u64`, including
zero. Domain-specific constructors and validators decide whether zero is a
valid sentinel, an unassigned value, or invalid for active runtime use.

| Type | Current owner | Rule |
| --- | --- | --- |
| `RequestId`, `SessionId` | `andromeda-types` | Correlation identifiers. Zero policy is domain-specific. |
| `TransactionId` | `andromeda-types` facade model | Transaction identity wrapper. Commit visibility still belongs to transaction and WAL rules. |
| `CatalogObjectId`, `CatalogVersion` | `andromeda-types` | Catalog validators reject zero for published object definitions and Procedure bindings. |
| `DatabaseId`, `InvocationId`, `NamespaceId`, `ProcedureId` | `andromeda-types` | Stable semantic IDs. Active Procedure bindings reject zero `ProcedureId`. |
| `ContractHash` | `andromeda-types` | 32-byte deterministic hash. Zero hash is reserved as "no binding" and is invalid for published contracts. |
| `StatsVersion` | `andromeda-contract` currently | Nonzero statistics binding evidence for Procedure contracts. Ownership is pending. |
| `PolicyVersion` | `andromeda-contract` currently | 32-byte digest-derived policy identity. Zero is invalid in active Procedure bindings. Ownership is pending. |

`Lsn` and `PageId` are durable-kernel identifiers. This spec does not move
them into `andromeda-types` or decide final ownership. Durable-kernel ownership
must stay aligned with WAL, page, storage, and recovery specs.

### StructuredObject ties

StructuredObject headers are the metadata-before-payload contract for typed
tabular payloads. A valid header must provide:

- non-empty name;
- nonzero owning `ContractHash`;
- nonzero descriptor hash;
- at least one field;
- `column_count == fields.len()`;
- dense, zero-based field ordinals;
- unique field names;
- validated field `TypeDescriptor` values;
- declared layout: `RowMajor`, `ColumnMajor`, or `Hybrid`;
- row-count policy and exact row-count evidence when required;
- payload length and optional maximum payload length;
- optional payload checksum evidence.

The descriptor hash includes:

- an explicit domain tag;
- layout tag;
- field count;
- each field name;
- each field ordinal;
- scalar family and scalar parameters;
- absence policy.

The descriptor hash excludes the StructuredObject name. Two differently named
StructuredObjects with identical shape and layout may share a descriptor hash,
while any scalar, absence, ordinal, field-name, or layout change must change
the descriptor hash.

### Procedure and ResultStream ties

Procedure contracts use `ColumnDescriptor` for inputs and ResultStream columns.
Validation must ensure:

- Procedure identity, catalog version, contract hash, stats version, and policy
  version are nonzero where active binding requires them;
- Procedure inputs may be empty only where the contract validator explicitly
  allows empty input lists;
- ResultStream columns are non-empty and valid;
- ResultStream cardinality is explicit: `One`, `OptionalOne`, `Many`, or
  `NonEmptyMany`;
- the legacy row-count exact flag remains the deterministic projection of full
  ResultStream cardinality until compatibility cleanup removes the legacy
  field.

Type validation and security admission must happen before transaction creation
when a contract, payload, or surface request can be rejected without a
transaction.

### Compatibility

Type compatibility must be explicit and hashable. The accepted v0 defaults are:

| Change | Default classification | Notes |
| --- | --- | --- |
| Add required Procedure input | Breaking | Existing callers cannot supply the new required value. |
| Add optional Procedure input with explicit stable default | Additive candidate | Requires compatibility policy and binder support. |
| Remove input or field | Breaking | Existing callers or consumers may depend on it. |
| Rename input, field, or ResultStream column | Breaking | Names are contract evidence. |
| Change scalar family | Breaking | Example: `I64` to `Text`, `Decimal` to `Float`. |
| Change integer width or signedness | Breaking unless a checked widening rule is specified | No silent conversion is allowed. |
| Change decimal precision or scale | Compatibility-relevant | Must be reviewed as breaking unless a specific widening policy is accepted. |
| Change float mode or width | Compatibility-relevant | Must not affect exact invariants. |
| Change text encoding, collation, or max length | Compatibility-relevant | Can change equality, ordering, payload bounds, or accepted values. |
| Change `AbsencePolicy` from `Required` to `ExplicitOptional` | Behavior-impacting | Existing consumers may not handle absence. Requires explicit compatibility policy. |
| Change `AbsencePolicy` from `ExplicitOptional` to `Required` | Breaking | Existing callers may omit the value. |
| Append ResultStream field under additive policy | Additive candidate | Requires named consumers and descriptor-hash evolution evidence. |
| Change StructuredObject layout | Compatibility-relevant | Layout participates in descriptor hash and payload interpretation. |
| Change ResultStream cardinality | Breaking or behavior-impacting | Especially `Many` to `One`, `One` to `OptionalOne`, or `Many` to `NonEmptyMany`. |

`ContractHash`, StructuredObject descriptor hashes, and catalog shape hashes
must include every type property that affects observable compatibility.

### Pending gaps

The following gaps remain open and must be tracked as explicit follow-up work:

| Gap | Current state | Required resolution |
| --- | --- | --- |
| Optional bool | Consolidated doctrine warns against nullable booleans because they create false three-state business logic. Current `TypeDescriptor::optional(ScalarType::Bool)` can be constructed unless a higher validator rejects it. | Decide whether optional bool is forbidden in `andromeda-types`, rejected by SRPL/catalog binders, or allowed only with a documented enum-or-optional policy. Until then, reviewers must treat optional bool as pending and require explicit justification. |
| `StatsVersion` ownership | Currently defined in `andromeda-contract`; roadmap names `andromeda-types`, `andromeda-policy`, catalog statistics, and optimizer consumers as related. | Decide whether `StatsVersion` remains contract-owned or moves to a statistics or foundation crate. Do not move it through this spec. |
| `PolicyVersion` ownership | Currently defined in `andromeda-contract`; roadmap names `andromeda-policy` as a future common policy descriptor owner. | Decide whether `PolicyVersion` remains contract-owned or moves to `andromeda-policy` once that crate exists. Do not move it through this spec. |
| `Lsn` ownership | `andromeda-wal` currently owns WAL primitives and LSN behavior after extraction, while storage keeps compatibility and recovery integration evidence. | Keep ownership with WAL/durable-kernel specs until a dedicated decision record changes it. |
| `PageId` ownership | Page and storage identifiers relate to storage-page, backup, manifest, and recovery plans. | Keep ownership with storage/page specs until a dedicated decision record changes it. |
| Binary scalar | Diagnostic JSON docs mention `Bytes`, but current `andromeda-types::ScalarType` does not define a binary scalar. | Add only through an explicit scalar-type decision, hash update, codec tests, and projection compatibility tests. |

## Validation

Documentation acceptance checks:

- The spec identifies `andromeda-types` as the R0 foundation owner for scalar
  descriptors and shared IDs.
- The spec identifies `andromeda-contract` and `andromeda-structured-object` as
  R1 contract owners with separate responsibilities.
- The spec rejects ad hoc SQL, implicit null semantics, runtime JSON defaults,
  native Rust layout serialization, and GPU authority in critical paths.
- The spec states that absence is modeled through `AbsencePolicy`, not ambient
  `NULL`.
- The spec documents decimals as exact and floats as controlled approximate
  values that cannot back exact transactional invariants.
- The spec ties StructuredObject descriptor hashing to scalar descriptors,
  absence policy, field names, ordinals, and layout.
- The spec marks optional bool, `StatsVersion`, `PolicyVersion`, `Lsn`, and
  `PageId` ownership as pending gaps instead of silently assigning ownership.

Future code validation should add or keep targeted tests for:

```powershell
cargo test -p andromeda-types --lib
cargo test -p andromeda-structured-object --lib
cargo test -p andromeda-contract --lib
cargo test -p andromeda-contract --test contract_hash_golden
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

Parser, binder, and catalog follow-up work should add invalid-case tests for:

- optional bool if the final policy rejects it;
- implicit-null predicate use;
- decimal precision and scale drift;
- float use in keys and exact invariants;
- text collation or length compatibility changes;
- StructuredObject descriptor-hash changes when scalar, absence, ordinal, field
  name, or layout changes.

These commands are not required for Worker 12B documentation-only acceptance.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Documentation says nullable values behave like SQL `NULL`. | Absence semantics drifted. | Reword to `AbsencePolicy` and explicit optional handling. |
| A float is used as a primary key, unique key, or money value. | Approximate values leaked into exact invariants. | Replace with decimal, integer, or a domain newtype. |
| A text comparison is contract-visible but no collation is declared. | Equality or ordering policy is implicit. | Add explicit collation or reject the comparison as contract-incomplete. |
| StructuredObject descriptor hash stays the same after a field type change. | Hash input omitted scalar or absence evidence. | Include scalar family, scalar parameters, absence policy, field name, ordinal, and layout in descriptor hashing. |
| Procedure binding accepts zero `ContractHash`, `StatsVersion`, or `PolicyVersion`. | Active binding validation is incomplete. | Reject before invocation or transaction creation. |
| JSON `nullable` is treated as runtime authority. | Diagnostic projection was confused with contract semantics. | Map it back to `AbsencePolicy` and validate against the Procedure or StructuredObject contract. |
| This spec is cited to move `Lsn` or `PageId`. | Ownership scope was overread. | Use WAL, page, storage, and recovery specs or create a decision record. |

## References

- `AGENTS.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `crates/README.md`
- `crates/andromeda-types/src/types.rs`
- `crates/andromeda-types/src/ids.rs`
- `crates/andromeda-contract/src/contracts/types.rs`
- `crates/andromeda-contract/src/objects.rs`
- `crates/andromeda-structured-object/src/lib.rs`
- `crates/andromeda-structured-object/src/hash.rs`
- `documentations/02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md`
- `documentations/reference/json-schema.md`
- `C:/Users/Arius/Desktop/andromeda_roadmap_restructuration_workspace_crates_engines_2026.md`
