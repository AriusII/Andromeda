# ContractHash Canonicalization v0 Specification

## Purpose

Define the accepted documentation contract for `ContractHash` canonicalization
as implemented by `andromeda-contract` and `andromeda-types`.

This specification documents the current Procedure contract hash algorithm,
field order, type tags, policy-version inputs, validation requirements, and
known gaps. The file name is `v0` because this is the first standalone
documentation baseline. The current implemented Procedure contract hash domain
string is `andromeda.catalog.procedure-contract.v4.sha256`.

## Scope

This specification applies to canonical hashes used for Procedure contracts and
policy versions in the Contract Plane.

It covers:

- `ContractHash` byte shape and sentinel rules;
- canonical hash sink primitives;
- Procedure contract hash domain string and input order;
- scalar type, absence, decimal, float, text, timestamp, and cardinality tags;
- policy-only digest inputs;
- validation and golden-test expectations;
- implemented versus pending gaps.

It also documents nearby object shape hashes where they affect Procedure
definition evidence, but it does not define durable catalog WAL bytes or RPC
wire payload bytes.

## Non-goals

This specification does not:

- define a general-purpose serialization format for all Andromeda objects;
- serialize Rust native structs directly to disk or network;
- define Protobuf wire order, QUIC frame bytes, WAL records, page layouts, or
  catalog WAL record bytes;
- introduce JSON as a runtime payload or canonical hash input;
- define cryptographic key management, signatures, or certificate trust;
- claim collision resistance beyond the current SHA-256 digest backend;
- decide future compatibility migration rules beyond the current hash domain
  strings and validation behavior;
- allow unordered canonicalization for fields whose current implementation is
  order-sensitive.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `crates/andromeda-contract/src/contracts/hash.rs`
- `crates/andromeda-contract/src/contracts/materialization.rs`
- `crates/andromeda-contract/src/contracts/types.rs`
- `crates/andromeda-contract/src/objects/shape_hash.rs`
- `crates/andromeda-digest/src/digest.rs`
- `crates/andromeda-types/src/ids.rs`
- `crates/andromeda-types/src/types.rs`
- `crates/andromeda-contract/tests/contract_hash_golden.rs`
- `crates/andromeda-srpl/tests/compiler_pipeline_e2e/contract_hash_canonical_forms.rs`
- `crates/andromeda-srpl/tests/definitionbatch_compat/source_digest.rs`
- `documentations/specs/ProcedureContract_v0.md`

## Procedure

### ContractHash representation

`ContractHash` is a 32-byte value. The all-zero value is a reserved sentinel
that means no active contract binding.

| Rule | Current behavior |
| --- | --- |
| Length | Exactly 32 bytes. |
| Construction from slice | Rejects any slice that is not exactly 32 bytes. |
| Display | Lowercase hexadecimal, 64 characters. |
| Debug display | `ContractHash(<lowercase-hex>)`. |
| Zero hash | Reserved sentinel; invalid for active Procedure contracts, protocol layout hashes, and full bindings. |
| Test vectors | `ContractHash::test_vector(byte)` builds repeated-byte fixtures for tests only. |

### Hash backend

The current canonical digest backend is `andromeda_digest::Sha256`. The
canonical sink appends typed bytes directly to SHA-256 and returns the 32-byte
digest as `ContractHash`.

The current implementation is deterministic and little-endian for integer
primitives. It is not yet exposed as a standalone public binary codec.

### Stable sink primitives

The current Procedure contract hash sink encodes primitive values as follows:

| Primitive | Encoding |
| --- | --- |
| `u8` | One raw byte. |
| `bool` | `0` for false, `1` for true. |
| `u32` | Four bytes, little-endian. |
| `u64` | Eight bytes, little-endian. |
| raw bytes | Appended with no length prefix. |
| string | UTF-8 bytes prefixed by `u64` byte length. |
| byte slice | Raw bytes prefixed by `u64` byte length. |
| `ContractHash` | 32 raw bytes with no additional length prefix. |
| qualified name | `u64` part count, then each part as a length-prefixed string. |
| columns | `u64` column count, then name, type descriptor, and ordinal for each column in vector order. |

Order is significant. The current implementation does not sort permissions,
error codes, inputs, structured inputs, or result streams during hashing.
Producers must provide the canonical order before materialization.

### Procedure contract hash

The current Procedure contract hash starts with the domain string:

```text
andromeda.catalog.procedure-contract.v4.sha256
```

The hash input order is:

| Order | Input | Encoding |
| ---: | --- | --- |
| 1 | Domain string | Length-prefixed string. |
| 2 | Procedure qualified name | Part count, then each normalized name part. |
| 3 | `StatsVersion` | `u64` little-endian value. |
| 4 | Protocol descriptor-set hash | 32 raw bytes. |
| 5 | Protocol frame-envelope hash | 32 raw bytes. |
| 6 | Input columns | Column list encoding. |
| 7 | Structured input count | `u64` count. |
| 8 | Structured input names | Each name as qualified-name encoding, in vector order. |
| 9 | Result stream count | `u64` count. |
| 10 | Result streams | Stream id, name, cardinality tag, and columns for each stream in vector order. |
| 11 | Required permission count | `u64` count. |
| 12 | Required permissions | Each permission as a length-prefixed string, in vector order. |
| 13 | Transaction access mode | Stable `u8` tag. |
| 14 | Transaction isolation | Stable `u8` tag. |
| 15 | Transaction retryable flag | Boolean. |
| 16 | Compatibility policy | Stable `u8` tag. |
| 17 | Result metadata policy | Stable `u8` tag. |
| 18 | Error rollback flag | Boolean. |
| 19 | Allowed error-code count | `u64` count. |
| 20 | Allowed error codes | Each code as a length-prefixed string, in vector order. |
| 21 | Multi-result policy | Stable `u8` tag. |

The following fields are intentionally not direct Procedure contract hash inputs
in the current implementation:

| Field | Reason |
| --- | --- |
| `ProcedureId` | Bound separately in `ProcedureContractBinding` and object shape hash. |
| Catalog object id | Bound separately in catalog object evidence and object shape hash. |
| `CatalogVersion` | Bound separately in `ProcedureContractBinding`; `ContractHash` can remain stable across catalog version advancement when the typed shape is unchanged. |
| Stored `contract_hash` | The hash is the output being validated. |
| Computed `PolicyVersion` | Derived from a subset of inputs, not an input to the full Procedure contract hash. |

### Stable tags

Current enum tags are part of the canonical hash contract.

| Enum | Value | Tag |
| --- | --- | ---: |
| `AccessMode` | `ReadOnly` | 0 |
| `AccessMode` | `ReadWrite` | 1 |
| `IsolationPolicy` | `Snapshot` | 0 |
| `IsolationPolicy` | `Serializable` | 1 |
| `CompatibilityPolicy` | `AdditiveOnly` | 0 |
| `CompatibilityPolicy` | `ExactHash` | 1 |
| `ResultMetadataPolicy` | `RequireBeforePayload` | 0 |
| `ResultMetadataPolicy` | `AllowStreamingUnknown` | 1 |
| `MultiResultPolicy` | `SingleResultOnly` | 0 |
| `MultiResultPolicy` | `MultipleResultStreamsAllowed` | 1 |
| `ResultStreamCardinality` | `One` | 0 |
| `ResultStreamCardinality` | `OptionalOne` | 1 |
| `ResultStreamCardinality` | `Many` | 2 |
| `ResultStreamCardinality` | `NonEmptyMany` | 3 |

### Type descriptor tags

Each column encodes its `TypeDescriptor` as scalar type followed by absence
policy.

| Type descriptor component | Value | Tag or encoding |
| --- | --- | --- |
| `AbsencePolicy` | `Required` | 0 |
| `AbsencePolicy` | `ExplicitOptional` | 1 |
| `ScalarType` | `I8` | 0 |
| `ScalarType` | `I16` | 1 |
| `ScalarType` | `I32` | 2 |
| `ScalarType` | `I64` | 3 |
| `ScalarType` | `I128` | 4 |
| `ScalarType` | `U8` | 5 |
| `ScalarType` | `U16` | 6 |
| `ScalarType` | `U32` | 7 |
| `ScalarType` | `U64` | 8 |
| `ScalarType` | `U128` | 9 |
| `ScalarType` | `Decimal` | 10, then decimal subtype. |
| `ScalarType` | `Float` | 11, then float subtype. |
| `ScalarType` | `Bool` | 12 |
| `ScalarType` | `Text` | 13, then text fields. |
| `ScalarType` | `Timestamp` | 14, then timestamp subtype. |

Decimal subtype tags:

| Decimal type | Tag or encoding |
| --- | --- |
| `Min` | 0 |
| `Mid` | 1 |
| `Max` | 2 |
| `Custom { precision, scale }` | 3, then `precision` as `u8`, then `scale` as `u8`. |

Float subtype tags:

| Float type | Tag or encoding |
| --- | --- |
| `Min` | 0 |
| `Mid` | 1 |
| `Max` | 2 |
| `Custom { bits, mode }` | 3, then `bits` as little-endian `u16`, then mode tag. |

Float mode tags:

| Float mode | Tag |
| --- | ---: |
| `Approximate` | 0 |
| `DeterministicAnalytics` | 1 |

Text fields are encoded after the `Text` scalar tag:

| Text field | Encoding |
| --- | --- |
| `encoding` | `Utf8` = 0, `Utf16` = 1, `Unicode` = 2. |
| `max_length` value | `u32`; absent is encoded as `0`. |
| `max_length` presence | Boolean. |
| `collation` presence | Boolean. |
| `collation` value | Length-prefixed string when present. |

Timestamp subtype tags:

| Timestamp type | Tag |
| --- | ---: |
| `Transaction` | 0 |
| `Invocation` | 1 |
| `MonotonicEpoch` | 2 |

### PolicyVersion digest

`PolicyVersion` is a 32-byte SHA-256 digest over the policy-relevant subset of
the Procedure contract. It is not the full `ContractHash`.

The current policy-version digest starts with the domain string:

```text
andromeda.catalog.policy-version.v1.sha256
```

The current policy-version input order is:

| Order | Input |
| ---: | --- |
| 1 | Domain string. |
| 2 | `StatsVersion`. |
| 3 | Required permission count. |
| 4 | Required permissions in vector order. |
| 5 | Transaction access mode. |
| 6 | Transaction isolation. |
| 7 | Transaction retryable flag. |
| 8 | Compatibility policy. |
| 9 | Result metadata policy. |
| 10 | Error rollback flag. |
| 11 | Allowed error-code count. |
| 12 | Allowed error codes in vector order. |
| 13 | Multi-result policy. |

The current policy-version digest intentionally excludes Procedure name,
protocol layout, input shape, structured input names, and result stream shape.
Two contracts with different shapes but identical policy-relevant fields produce
the same `PolicyVersion`.

### Object shape hashes

`andromeda-contract` also implements object shape hashes. They are related
catalog evidence but are not the Procedure contract hash.

| Shape hash | Domain string | Main inputs |
| --- | --- | --- |
| Table shape | `andromeda.catalog.table-shape.v2.sha256` | Object reference and columns. |
| StructuredObject shape | `andromeda.catalog.structured-object-shape.v2.sha256` | Object reference, fields, and `unique_by`. |
| Enum shape | `andromeda.catalog.enum-shape.v2.sha256` | Object reference and variants. |
| Procedure definition shape | `andromeda.catalog.procedure-definition-shape.v1.sha256` | Object reference, `ProcedureId`, `ContractHash`, `StatsVersion`, and `PolicyVersion`. |

Object reference shape hashing includes catalog object id, catalog version,
object kind tag, and qualified name. This differs from the full Procedure
contract hash, which hashes the Procedure qualified name but not the catalog
object id or catalog version.

### Canonicalization boundaries

Canonicalization is currently field-order preserving. These fields are
order-sensitive:

- input columns;
- structured inputs;
- result streams;
- result stream columns;
- required permissions;
- allowed error codes;
- qualified name parts.

Current tests explicitly prove that reordering required permissions or allowed
error codes changes both `ContractHash` and `PolicyVersion`.

Equivalent SRPL source formatting can still produce the same contract hash
because the SRPL pipeline lowers source text to a canonical typed contract
shape before materialization. Exact SRPL source text has separate source digest
evidence and must not be conflated with `ContractHash`.

## Validation

Documentation acceptance checks:

- The spec states that `ContractHash` is 32 bytes and all-zero is reserved.
- The spec documents the current implemented Procedure contract hash domain
  string `andromeda.catalog.procedure-contract.v4.sha256`.
- The spec documents the current policy-version domain string
  `andromeda.catalog.policy-version.v1.sha256`.
- The spec distinguishes the standalone documentation baseline `v0` from the
  implemented hash domain version strings.
- The spec documents field order and stable enum tags without inventing a
  different canonical format.
- The spec states that current hash canonicalization is order-sensitive for
  ordered vectors.
- The spec states that no Rust native struct serialization, JSON runtime
  payload, gRPC surface, or ad hoc SQL input is part of canonicalization.
- The spec lists implemented and pending gaps.

Current implementation gates:

```powershell
cargo test -p andromeda-types ids
cargo test -p andromeda-contract --test contract_hash_golden
cargo test -p andromeda-srpl --test compiler_pipeline_e2e
cargo test -p andromeda-srpl --test definitionbatch_compat
```

These commands are recommended for code changes that modify hashing,
materialization, type tags, or SRPL lowering. For this documentation-only
worker, source cross-check and Markdown inspection are sufficient unless a wider
repository gate is requested.

### Validation matrix

| Scenario | Expected result | Current evidence |
| --- | --- | --- |
| `ContractHash::from_slice()` receives 31 bytes. | Rejected as `Contract` error. | Implemented in `andromeda-types` tests. |
| `ContractHash` is displayed. | Lowercase 64-character hex string. | Implemented in `andromeda-types` tests. |
| All-zero `ContractHash` appears in active Procedure contract. | Rejected by contract validation. | Implemented by `ProcedureContractRef::validate()` and binding validation. |
| Candidate materializes with stable fields. | Canonical hash is stored and validates. | Implemented by `ProcedureContractCandidate::materialize()`. |
| Contract field changes after hash computation. | `validate_canonical_hash()` rejects the contract. | Covered by contract golden tests. |
| Required permissions are reordered. | `ContractHash` and `PolicyVersion` change. | Covered by contract golden tests. |
| Allowed error codes are reordered. | `ContractHash` and `PolicyVersion` change. | Covered by contract golden tests. |
| Equivalent SRPL formatting lowers to same typed shape. | `ContractHash` remains stable. | Covered by SRPL DefinitionBatch tests. |
| Exact SRPL source text changes but typed shape is equivalent. | Source digest changes, `ContractHash` remains stable. | Covered by SRPL DefinitionBatch tests. |
| Semantic SRPL shape changes. | `ContractHash` changes. | Covered by SRPL DefinitionBatch tests. |

## Current Implementation Status

Implemented:

- `ContractHash` is a 32-byte newtype with exact slice validation, lowercase
  hex display, and an all-zero sentinel.
- Procedure contract hashes use SHA-256, stable domain string, explicit
  little-endian integer encoding, length-prefixed strings, raw 32-byte hash
  inputs, stable enum tags, and ordered vector traversal.
- `ProcedureContractCandidate::materialize()` computes and validates the
  canonical hash before returning a Procedure contract.
- `ProcedureContract::validate_canonical_hash()` rejects stale stored hashes.
- `PolicyVersion` is computed from a policy-only digest and included in full
  Procedure binding evidence.
- Object shape hashes exist for table, StructuredObject, enum, and Procedure
  definition evidence.
- Golden tests cover stale hashes and ordered permission/error-code behavior.
- SRPL tests cover stable canonical Procedure hashes across equivalent source
  formatting and separation from exact source digest evidence.

Pending or partial:

- The stable hash sink is internal, not a standalone public canonical binary
  codec.
- The project still needs a dedicated compatibility plane spec for future
  algorithm migration, downgrade handling, and multi-version hash acceptance.
- The current Procedure contract hash uses domain `v4`; earlier domain versions
  are not documented here as active compatibility inputs.
- StructuredObject payload canonicalization, RPC payload canonicalization, WAL
  records, pages, manifests, and catalog WAL records are outside this spec.
- There is no documented cross-language canonicalization test vector table with
  full byte transcript yet; current evidence is Rust tests.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Two semantically identical Procedure contracts have different hashes. | Ordered fields differ, such as permission order, error-code order, result stream order, or column order. | Canonicalize producer order before materialization or document the change as intentionally incompatible. |
| `PolicyVersion` changes when only permission order changes. | Permission order is currently hash-significant. | Preserve canonical permission order in producers. |
| `ContractHash` stays stable across catalog version advancement. | `CatalogVersion` is not a direct full Procedure contract hash input. | Validate `ProcedureContractBinding` when catalog-version identity matters. |
| Source digest changes but contract hash does not. | Exact SRPL text changed without changing the canonical typed contract shape. | Treat source digest and `ContractHash` as separate evidence classes. |
| A contract validates structurally but fails canonical hash validation. | Stored `contract_hash` is stale. | Re-materialize from `ProcedureContractCandidate` or recompute the hash before publication. |
| Documentation describes a hash over serialized Rust structs. | Boundary drift. | Reword to explicit canonical hash sink inputs and stable tags. |

## References

- `crates/andromeda-contract/src/contracts/hash.rs`
- `crates/andromeda-contract/src/contracts/materialization.rs`
- `crates/andromeda-contract/src/contracts/types.rs`
- `crates/andromeda-contract/src/objects/shape_hash.rs`
- `crates/andromeda-digest/src/digest.rs`
- `crates/andromeda-types/src/ids.rs`
- `crates/andromeda-types/src/types.rs`
- `crates/andromeda-contract/tests/contract_hash_golden.rs`
- `crates/andromeda-srpl/tests/compiler_pipeline_e2e/contract_hash_canonical_forms.rs`
- `crates/andromeda-srpl/tests/definitionbatch_compat/source_digest.rs`
- `documentations/specs/ProcedureContract_v0.md`
