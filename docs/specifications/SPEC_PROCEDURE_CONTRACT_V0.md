# Specification: ProcedureContract v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `ProcedureContract v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define the canonical contract for callable Procedures.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.
- `PlanClass` is intentionally NOT part of the procedure-contract chain in V0; it is owned by the plan-cache key — see `docs/specifications/SPEC_PLAN_CACHE_KEY_V0.md`.

## Data structures

| Structure | Required role |
|---|---|
| `ProcedureContract` | Must be represented as an explicit typed structure or canonical descriptor. |
| `InputShape` | Must be represented as an explicit typed structure or canonical descriptor. |
| `StructuredObjectContract` | Must be represented as an explicit typed structure or canonical descriptor. |
| `OutputShape` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RequiredPermissions` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ReadSet` | Must be represented as an explicit typed structure or canonical descriptor. |
| `WriteSet` | Must be represented as an explicit typed structure or canonical descriptor. |
| `IsolationPolicy` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ResourcePolicy` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ProtocolLayout` | Must be represented as an explicit typed structure or canonical descriptor. |
| `CompatibilityPolicy` | Must be represented as an explicit typed structure or canonical descriptor. |
| `CanonicalContractShape` | Ordered canonical descriptor hashed into `ContractHash`. |
| `ContractRejectionCode` | Stable typed rejection code for failed admission or publication. |

### ProcedureContract v0 required fields

`ProcedureContract` must contain these normalized fields before publication:

| Field | Rule |
|---|---|
| `object` | `CatalogObjectRef` whose kind is `Procedure`, whose `CatalogObjectId` and `CatalogVersion` are non-zero, and whose qualified name is the durable procedure name. |
| `ProcedureId` | Non-zero stable procedure identity. It is binding evidence and is intentionally not a `ContractHash` input. |
| `ContractHash` | Non-zero canonical SHA-256 digest of `CanonicalContractShape`. |
| `StatsVersion` | Non-zero version of statistics evidence used by the contract. |
| `ProtocolLayout` | Non-zero descriptor-set hash and non-zero frame-envelope hash. |
| `InputShape` | Ordered input columns with stable names, explicit types, explicit absence policy, dense zero-based ordinals, and no duplicate names. |
| `StructuredObjectContract` | Ordered structured input names and object bindings. Names must be unique. |
| `OutputShape` | Ordered result streams. Each stream has non-zero stream id, stable name, explicit cardinality, and ordered columns with dense zero-based ordinals. |
| `RequiredPermissions` | Non-empty unique permission identifiers sorted or otherwise canonicalized before hashing. Unknown or ambient permissions are invalid. |
| `ReadSet` and `WriteSet` | Explicit object references derived by binder/catalog resolution. Runtime branches must not create hidden read/write sets. |
| `IsolationPolicy` | Explicit transaction isolation and access mode. |
| `ResourcePolicy` | Explicit retry, timeout, quota, backpressure, and budget policy when the runtime enforces any of them. Hidden runtime defaults are invalid. |
| `CompatibilityPolicy` | `AdditiveOnly` or `ExactHash` in V0. |
| `ResultMetadataPolicy` | Explicit metadata-before-payload decision. |
| `ProcedureErrorPolicy` | Explicit rollback-on-error flag and stable allowed error code list. |
| `MultiResultPolicy` | Explicit single-result or multiple-result-stream policy. |

`ProcedureContractBinding` is the pre-transaction invocation identity. It must carry
non-zero `ProcedureId`, `CatalogVersion`, `ContractHash`, `StatsVersion`, and `PolicyVersion`.
`ProcedureContractRef` alone is legacy compatibility evidence and must not be accepted for new
admission paths that can observe statistics or policy drift.

## Invariants

- ContractHash is deterministic.
- ContractHash is based on canonical shape, not raw source text.
- Canonical shape includes InputShape, OutputShape, ReadSet, WriteSet, RequiredPermissions, IsolationPolicy, ResourcePolicy, ProtocolLayout, and CompatibilityPolicy.
- Required permissions are part of the contract.
- Contract compatibility is explicit.
- Metadata precedes ResultStream payload.
- Contract rejection codes are stable enough for tests and audit evidence.
- `ProcedureId`, `CatalogObjectId`, `CatalogVersion`, `ContractHash`, `StatsVersion`, and `PolicyVersion` are all non-zero wherever they appear in binding evidence.
- `ContractHash` changes when observable procedure shape, protocol, permission, policy, result metadata, error, multi-result, or statistics evidence changes.
- `ContractHash` does not by itself prove stable `ProcedureId`; DefinitionBatch source evidence or catalog binding evidence must detect `ProcedureId` drift.
- Result stream cardinality is explicit: `One`, `OptionalOne`, `Many`, or `NonEmptyMany`. The legacy row-count projection must be a deterministic projection of cardinality.
- Output columns are identified by stable name and dense ordinal; positional-only result shapes are invalid.
- `PolicyVersion` is a separate digest of policy-relevant execution fields and must be validated alongside `ContractHash`.
- Read/write sets must be derived before publication and must not depend on request data, runtime branch selection, dynamic SQL text, or implementation-specific reflection.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### Canonical contract hash form

`ContractHash` is a hash of canonical typed contract shape, not source text and not native layout.

The V0 canonical hash domain is `andromeda.catalog.procedure-contract.v4.sha256`. The digest
algorithm is SHA-256. All integer fields in the canonical hash stream use little-endian fixed-width
encoding. Strings and variable byte fields are length-prefixed with `u64` byte length before UTF-8
or raw bytes. Native Rust struct layout, debug formatting, map iteration order, and source text are
not valid hash inputs.

| Canonical input group | Required content |
|---|---|
| Domain separator | Stable `ProcedureContractV0` domain and format version. |
| Identity shape | Qualified procedure name and object kind; `ProcedureId` is binding evidence, not a semantic hash substitute. |
| Inputs | Ordered `InputShape` descriptors with type, absence policy, cardinality, and stable names. |
| Outputs | Ordered `OutputShape` and ResultStream descriptors with metadata policy and column shapes. |
| Permissions | Sorted `RequiredPermissions` and surface constraints. |
| Effects | `ReadSet`, `WriteSet`, `IsolationPolicy`, and `ResourcePolicy`. |
| Protocol | `ProtocolLayout`, result metadata policy, error policy, and multi-result policy. |
| Compatibility | `CompatibilityPolicy` and explicit default values. |

Invocation binding must carry `ProcedureId`, `CatalogVersion`, `ContractHash`, `StatsVersion`, and `PolicyVersion`. A `ProcedureContractRef` alone is insufficient for pre-transaction admission.

The canonical field order is:

1. Domain string.
2. Qualified procedure name.
3. `StatsVersion`.
4. `ProtocolLayout.descriptor_set_hash`.
5. `ProtocolLayout.frame_envelope_hash`.
6. Input columns.
7. Structured input qualified names.
8. Result streams with stream id, name, cardinality tag, and columns.
9. Required permissions.
10. Transaction policy: access mode, isolation, retryable flag.
11. Compatibility policy.
12. Result metadata policy.
13. Error policy: rollback flag and allowed error codes.
14. Multi-result policy.

The V0 stable tags are:

| Enum | Value |
|---|---|
| `AccessMode.ReadOnly` | `0` |
| `AccessMode.ReadWrite` | `1` |
| `IsolationPolicy.Snapshot` | `0` |
| `IsolationPolicy.Serializable` | `1` |
| `CompatibilityPolicy.AdditiveOnly` | `0` |
| `CompatibilityPolicy.ExactHash` | `1` |
| `ResultMetadataPolicy.RequireBeforePayload` | `0` |
| `ResultMetadataPolicy.AllowStreamingUnknown` | `1` |
| `ResultStreamCardinality.One` | `0` |
| `ResultStreamCardinality.OptionalOne` | `1` |
| `ResultStreamCardinality.Many` | `2` |
| `ResultStreamCardinality.NonEmptyMany` | `3` |
| `MultiResultPolicy.SingleResultOnly` | `0` |
| `MultiResultPolicy.MultipleResultStreamsAllowed` | `1` |

Type descriptors in the hash must use the canonical TypeSystem encoding. Absence is explicit:
required and explicit-optional are distinct; ambient NULL is not a contract state.

### PolicyVersion form

`PolicyVersion` is a SHA-256 digest with domain `andromeda.catalog.policy-version.v1.sha256`.
It includes `StatsVersion`, `RequiredPermissions`, transaction policy, compatibility policy,
result metadata policy, error policy, and multi-result policy. It intentionally excludes result
columns and structured input shape so policy drift can be detected independently from full
contract shape drift.

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

### ContractRejectionCode v0

Implementations must expose stable typed rejection codes or stable diagnostic ids for these V0
failures. Message text may change; the code must not change without a versioned compatibility
decision.

| Code | Required use |
|---|---|
| `CONTRACT_HASH_MISSING` | Contract or binding has a zero or absent `ContractHash`. |
| `CONTRACT_HASH_NON_CANONICAL` | Stored hash differs from canonical `ProcedureContract` shape. |
| `CONTRACT_HASH_SOURCE_TEXT_DERIVED` | Hash evidence is derived from raw SRPL/source text instead of canonical shape. |
| `PROCEDURE_ID_MISSING` | `ProcedureId` is zero or absent in contract/binding evidence. |
| `CATALOG_VERSION_MISSING` | `CatalogVersion` is zero or absent where invocation or publication requires it. |
| `STATS_VERSION_MISSING` | `StatsVersion` is zero or absent in contract/binding evidence. |
| `POLICY_VERSION_MISSING` | `PolicyVersion` is zero or absent in binding evidence. |
| `PROTOCOL_LAYOUT_HASH_MISSING` | Descriptor-set or frame-envelope hash is zero. |
| `INPUT_SHAPE_DRIFT` | Input columns or structured inputs changed under a policy that forbids it. |
| `OUTPUT_SHAPE_DRIFT` | Existing result stream id, cardinality, or existing column prefix changed. |
| `PERMISSION_DRIFT` | Required permission set changed under a policy that forbids it. |
| `POLICY_DRIFT` | Transaction, result metadata, error, multi-result, resource, or policy digest changed unexpectedly. |
| `RUNTIME_DEPENDENT_SHAPE` | Any input, output, read/write set, or permission shape depends on runtime branches. |
| `UNKNOWN_REQUIRED_PERMISSION` | Permission identifier is not known to the active policy/catalog. |
| `UNSTABLE_REJECTION_CODE` | A rejection path lacks a stable typed code. |

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

| Compatibility case | V0 decision |
|---|---|
| Add optional output field with explicit default | Additive. |
| Add required input | Breaking. |
| Remove result stream or existing output column | Breaking. |
| Change existing output cardinality | Breaking. |
| Change required permission | Security-impact and denied by default. |
| Change isolation, resource, protocol, or error policy | Review-required; denied unless explicitly versioned. |
| Change only source formatting with same canonical shape | Compatible. |

### Procedure compatibility decision matrix

The V0 decision engine is owned by `andromeda-procedure-contract`. A taxonomy-only crate may name
classes, but catalog publication must call a concrete Procedure compatibility classifier before
publishing an updated Procedure descriptor.

| Check | `ExactHash` | `AdditiveOnly` |
|---|---|---|
| Same qualified procedure name | Required. | Required. |
| Same `ProcedureId` | Required. | Required. |
| Same `CatalogObjectId` | Required. | Required. |
| Object kind is `Procedure` | Required. | Required. |
| Advancing `CatalogVersion` | Required. | Required. |
| Canonical `ContractHash` | Must be unchanged. | May change only for allowed additive output growth. |
| Inputs and structured inputs | Must be unchanged through hash equality. | Must be unchanged. |
| Required permissions | Must be unchanged through hash equality. | Must be unchanged; any change is security-impact and denied by default. |
| `StatsVersion` | Must be unchanged through hash equality. | Must be unchanged. |
| `ProtocolLayout` | Must be unchanged through hash equality. | Must be unchanged. |
| Transaction/resource policy | Must be unchanged through hash equality. | Must be unchanged. |
| Result metadata, error, multi-result policy | Must be unchanged through hash equality. | Must be unchanged. |
| Existing result stream | Must be unchanged through hash equality. | Stream id, name, cardinality, and existing column prefix must be unchanged. |
| New result stream | Rejected because hash changes. | Allowed only when `MultiResultPolicy` allows multiple streams and metadata policy remains valid. |
| New output column | Rejected because hash changes. | Allowed only as an appended optional column with explicit default and unchanged existing prefix. |

Any compatibility diagnostic that affects publication must be retained as DefinitionBatch/catalog
evidence with previous and next `CatalogVersion`, previous and next `ContractHash`, policy class,
and stable rejection code when rejected.

> **Note — `PolicyVersion` disambiguation:** The `PolicyVersion` field in this contract is the **policy digest** (SHA-256 over policy-relevant fields; see `## Serialization` → `PolicyVersion form`). The monotonic policy snapshot identifier used by the admission layer and the security-loaded policy digest are distinct values that share the same label; see `docs/adr/ADR-0019-POLICY_VERSION_TERMINOLOGY.md` for the canonical disambiguation.

## Tests

- contract hash golden tests.
- canonical byte stream golden tests for `andromeda.catalog.procedure-contract.v4.sha256`.
- policy version golden tests for `andromeda.catalog.policy-version.v1.sha256`.
- canonical shape field-order tests.
- additive change compatibility tests.
- breaking change rejection tests.
- payload shape mismatch tests.
- stable ContractRejectionCode tests.
- non-zero binding evidence tests for `ProcedureId`, `CatalogVersion`, `ContractHash`, `StatsVersion`, and `PolicyVersion`.
- result stream cardinality stable-tag and legacy-projection tests.
- read/write set determinism tests proving no runtime branch can shape the contract.

## Rejection criteria

- Reject `missing ContractHash`.
- Reject `ContractHash derived from raw source text`.
- Reject `unknown required permission`.
- Reject `shape depends on runtime branch`.
- Reject `output columns are positional only`.
- Reject `unstable contract rejection code`.
- Reject `ProcedureContractBinding missing ProcedureId CatalogVersion ContractHash StatsVersion or PolicyVersion`.
- Reject `stored ContractHash that does not equal canonical contract hash`.
- Reject `Procedure compatibility without advancing CatalogVersion`.
- Reject `AdditiveOnly input structured-input permission policy stats protocol transaction result-metadata error or multi-result drift`.
- Reject `result stream removal id drift cardinality drift or changed existing column prefix`.
- Reject `new result stream when MultiResultPolicy is SingleResultOnly`.
- Reject `ambient NULL or implicit optional semantics in InputShape or OutputShape`.

## Implementation handoff

P02/P03/P04 implementation work must use this handoff:

| Owner crate or boundary | Required responsibility |
|---|---|
| `andromeda-procedure-contract` | Owns `ProcedureContract`, canonical `ContractHash`, `PolicyVersion`, binding validation, and Procedure compatibility decisions. |
| SRPL binder/lowering | Produces deterministic `InputShape`, structured inputs, result streams, read/write sets, and permission requirements before publication. |
| `andromeda-catalog` | Publishes `ProcedureDescriptor` only with accepted compatibility evidence and a durable `CatalogVersion`. |
| DefinitionBatch | Carries source evidence that detects `ProcedureId` drift even when `ContractHash` is unchanged. |
| Execution admission | Rejects invocation before transaction creation unless `ProcedureId`, `CatalogVersion`, `ContractHash`, `StatsVersion`, and `PolicyVersion` match catalog evidence. |
| Protocol/RPC | Uses `ProtocolLayout` and ResultStream metadata policy; it must not infer shapes from payload contents. |

## Acceptance summary

Owner: `andromeda-procedure-contract` owns `ProcedureContract`, canonical `ContractHash`,
`PolicyVersion`, binding validation, and Procedure compatibility classification; `andromeda-catalog`
and DefinitionBatch consume that evidence for publication.

Evidence: acceptance requires golden tests for canonical contract bytes and policy bytes, additive
and breaking compatibility tests, non-zero binding evidence tests, ResultStream cardinality/tag
tests, and catalog handoff evidence showing invocation binding carries `ProcedureId`,
`CatalogVersion`, `ContractHash`, `StatsVersion`, and `PolicyVersion`.

Reject: acceptance is not met if any implementation accepts raw-source-derived hashes,
non-canonical stored hashes, missing binding evidence, runtime-dependent shapes, permission or
policy drift without a typed decision, positional-only outputs, ambient NULL semantics, or a
rejection path without stable `ContractRejectionCode` evidence.
