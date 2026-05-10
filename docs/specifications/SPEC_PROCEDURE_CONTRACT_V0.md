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

## Invariants

- ContractHash is deterministic.
- ContractHash is based on canonical shape, not raw source text.
- Canonical shape includes InputShape, OutputShape, ReadSet, WriteSet, RequiredPermissions, IsolationPolicy, ResourcePolicy, ProtocolLayout, and CompatibilityPolicy.
- Required permissions are part of the contract.
- Contract compatibility is explicit.
- Metadata precedes ResultStream payload.
- Contract rejection codes are stable enough for tests and audit evidence.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### Canonical contract hash form

`ContractHash` is a hash of canonical typed contract shape, not source text and not native layout.

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

| Compatibility case | V0 decision |
|---|---|
| Add optional output field with explicit default | Additive. |
| Add required input | Breaking. |
| Remove result stream or existing output column | Breaking. |
| Change existing output cardinality | Breaking. |
| Change required permission | Security-impact and denied by default. |
| Change isolation, resource, protocol, or error policy | Review-required; denied unless explicitly versioned. |
| Change only source formatting with same canonical shape | Compatible. |

## Tests

- contract hash golden tests.
- canonical shape field-order tests.
- additive change compatibility tests.
- breaking change rejection tests.
- payload shape mismatch tests.
- stable ContractRejectionCode tests.

## Rejection criteria

- Reject `missing ContractHash`.
- Reject `ContractHash derived from raw source text`.
- Reject `unknown required permission`.
- Reject `shape depends on runtime branch`.
- Reject `output columns are positional only`.
- Reject `unstable contract rejection code`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
