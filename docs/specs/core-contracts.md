# Core Contracts

## Purpose

This spec defines the shared contract layer for typed Procedure invocation,
catalog binding, canonical hashes, StructuredObject payloads, and compatibility
classification. It does not define storage truth, WAL truth, security policy
runtime behavior, or execution semantics.

## Ownership

| Area | Canonical owner |
| --- | --- |
| Scalar descriptors, semantic identifiers, and `ContractHash` primitives | Foundation type crates. |
| Procedure contracts, bindings, compatibility diagnostics, qualified names, and catalog object descriptors | Contract owner crate. |
| StructuredObject headers, layout descriptors, descriptor hashes, and row-count policy | StructuredObject owner crate. |
| Protocol layout identity and ResultStream projection | Protocol schema and RPC contract owners. |

Compatibility facades may preserve historical imports, but a facade must not
be treated as the canonical owner of new behavior.

## Type system

| Family | Rule |
| --- | --- |
| Signed and unsigned integers | Width and signedness are explicit. Silent widening, narrowing, and signed/unsigned conversion are not allowed. |
| Decimal | Exact numeric family. Custom precision must be positive and scale must not exceed precision. |
| Float | Approximate or analytics family. Float values must not decide catalog publication, WAL durability, recovery truth, security admission, or exact identifiers. |
| Bool | Two-state boolean. Optional booleans are pending policy and require explicit justification until resolved. |
| Text | Encoding is explicit. Length and collation are part of compatibility when comparison or bounds matter. |
| Timestamp | Source is explicit: transaction, invocation, or monotonic epoch. Ambient wall-clock time is not a contract-stable value source. |

Absence is represented by `AbsencePolicy`. `Required` rejects missing values.
`ExplicitOptional` permits absence only when callers and SRPL handling account
for the absent branch. SQL-style implicit null behavior is not part of the
contract model.

## Semantic identifiers

Active bindings must reject zero values for `ProcedureId`, `CatalogObjectId`,
`CatalogVersion`, `StatsVersion`, `PolicyVersion`, and `ContractHash`.
`ContractHash` is a deterministic 32-byte identity. The all-zero hash is
reserved for "no binding" and is invalid for published Procedure contracts.

`Lsn` ownership remains with WAL and durable-kernel specs. `PageId` ownership
remains with storage and page specs. Moving either type requires a dedicated
decision.

## Procedure contract

An active `ProcedureContract` must validate these fields before invocation,
plan-cache lookup, audit evidence, or transaction creation:

| Field | Required rule |
| --- | --- |
| `object` | Nonzero catalog object reference of kind `Procedure` with nonzero `CatalogVersion`. |
| `procedure_id` | Nonzero and stable for the Procedure object. |
| `contract_hash` | Nonzero and equal to the canonical hash of the contract. |
| `stats_version` | Nonzero when planning, binding, or optimizer evidence depends on statistics. |
| `protocol_layout` | Includes nonzero descriptor-set and frame-envelope hashes. |
| `inputs` | Dense zero-based ordinals; each column validates scalar and absence policy. |
| `structured_inputs` | Unique qualified names. |
| `result_streams` | Unique stream ids and names; each stream has valid columns and cardinality. |
| `required_permissions` | Non-empty, non-empty entries, unique under the canonical ordering used by the producer. |
| `transaction_policy` | Explicit access mode, isolation, and retryability. |
| `compatibility_policy` | `ExactHash` or `AdditiveOnly`. |
| `result_metadata_policy` | Explicit metadata-before-payload behavior. |
| `error_policy` | Rollback flag is explicit; allowed error codes are unique when present. |
| `multi_result_policy` | Agrees with the number of result streams. |

Result stream cardinality uses stable tags:

| Cardinality | Minimum | Maximum | Legacy exact row count |
| --- | ---: | --- | --- |
| `One` | 1 | 1 | Required. |
| `OptionalOne` | 0 | 1 | Not required. |
| `Many` | 0 | Unbounded | Not required. |
| `NonEmptyMany` | 1 | Unbounded | Required. |

## Canonical hashing

Canonical hashes are computed from explicit sink inputs and stable tags, not
from serialized Rust structs. Multi-byte integer inputs use the specified
codec order for the relevant hash sink. Ordered fields are hash-significant
unless a producer has already canonicalized their order before materialization.

`ContractHash` and `PolicyVersion` must change when required permissions,
allowed error codes, result streams, columns, transaction policy, result
metadata policy, multi-result policy, or protocol layout change in a
hash-significant way. SRPL source text digest and `ContractHash` are separate
evidence classes: equivalent typed shape can keep the same contract hash while
source digest changes.

## StructuredObject payloads

StructuredObject payload interpretation is governed by descriptor hashes,
field names, ordinals, scalar descriptors, absence policies, row-count policy,
and layout evidence. JSON projections, generated protocol messages, and
debug text are diagnostic or transport views; they must not become durable
payload truth.

Payload readers must validate descriptor identity before interpreting fields.
Any layout or type change that affects payload interpretation is
compatibility-relevant and must update the descriptor hash or be rejected.

## Compatibility classes

| Class | Meaning | Default effect |
| --- | --- | --- |
| `Unchanged` | Compatibility-relevant evidence is unchanged. | Existing bindings may remain valid when full binding evidence still matches. |
| `Additive` | Compatible surface is added without invalidating existing callers. | New callers bind new evidence; existing compatible callers can continue. |
| `Breaking` | Identity, invocation shape, result shape, permission, policy, dependency, or binding evidence changed unsafely. | New binding or explicit migration is required. |
| `Deprecated` | Active visibility ended while historical evidence remains. | New invocations must not bind to the deprecated version. |
| `Rejected` | Validation, compatibility, dependency, or durability checks failed. | No visible catalog or invocation change is accepted. |

`ExactHash` accepts only an exact canonical hash match. `AdditiveOnly` is
intentionally narrow: it may accept appended result columns or added result
surface only when existing result stream identity, column prefix, inputs,
permissions, policies, protocol layout, and binding evidence remain valid.

## Validation gates

- Reject stale or zero contract identity before transaction creation.
- Reject stored hashes that do not match canonical hash recomputation.
- Reject partial Procedure bindings that omit catalog version, contract hash,
  stats version, or policy version when the target path requires them.
- Reject raw payload bodies in diagnostics; emit bounded codes, lengths,
  digests, and typed identifiers.
- Reject documentation or code that treats SQL signatures, native layout, or
  diagnostic JSON as the Procedure contract.

## Open decisions

- Optional boolean policy remains unresolved.
- `StatsVersion` and `PolicyVersion` ownership remains pending.
- A binary scalar requires an explicit scalar decision, hash update, codec
  tests, and compatibility tests.
