# Specification: AuditLedger v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `AuditLedger v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define durable audit evidence.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `AuditRecord` | Must be represented as an explicit typed structure or canonical descriptor. |
| `AuditChainHash` | Must be represented as an explicit typed structure or canonical descriptor. |
| `AuditPolicy` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RetentionPolicy` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ExportEvidence` | Must be represented as an explicit typed structure or canonical descriptor. |
| `AuditRecordHeader` | Fixed-width header with magic, version, sequence, length, and CRC. |
| `AuditRecordPayload` | Typed redacted payload for security, admin, catalog, recovery, or HA/DR event families. |
| `AuditLedgerVersion` | Explicit durable audit format version. |
| `AuditRejectionCode` | Stable typed rejection code for invalid append, replay, or export. |

## Invariants

- Audit is append-only.
- Audit record magic and version are validated before replay.
- Critical audit cannot be globally disabled.
- Records contain principal, certificate, surface, operation, result, and policy version.
- Deletion is detectable.
- ChainHash binds record order and detects gaps, deletion, reorder, and duplicate sequence numbers.
- Secret values are redacted before persistence.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### Durable audit record format

V0 durable audit records are ASCII field records with explicit version prefix, key/value fields, and checksum suffixes. Binary replacement remains a future version and must keep the same validation semantics.

| Field | Required rule |
|---|---|
| Format prefix | Must be a supported audit journal version. |
| RecordLsn | Strictly increasing durable audit record LSN. |
| DurableLsn | WAL or sink durability evidence for the record. |
| EventId | Non-zero event identity. |
| TraceId | Non-zero trace identity. |
| Family | Known audit family. |
| Sequence | Strict per-family sequence evidence when applicable. |
| Retention | Retain, compactable, or export-retained policy. |
| Replay | Replay behavior classification. |
| PrincipalId | Redacted stable principal binding. |
| CertificateFingerprint | Optional redacted certificate evidence. |
| Surface and Permission | Optional but required for permissioned security events. |
| PolicyVersion and PolicyDigest | Required for policy-backed security decisions. |
| RequestId and SessionId | Required when tied to an RPC request. |
| EventKind | Stable event vocabulary. |
| PreviousChainChecksum | Must equal the prior chain checksum. |
| ChainChecksum | Checksum of previous chain, record checksum, and payload. |
| Checksum | Non-zero checksum of the record payload. |

Replay rejects missing checksum fields, non-ASCII payloads, wrong field counts, checksum mismatch, chain mismatch, duplicate/non-monotonic LSN, and broken retention compaction proof.

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

- append chain tests.
- tamper detection tests.
- retention policy tests.
- export tests.
- bad magic and unsupported version tests.
- redaction and secret-leak rejection tests.
- stable AuditRejectionCode tests.
- replay chain mismatch tests.
- duplicate and reordered record tests.

## Rejection criteria

- Reject `audit disable switch`.
- Reject `string-only audit`.
- Reject `record without policy version`.
- Reject `audit record with secret payload`.
- Reject `audit record without chain hash`.
- Reject `unstable audit rejection code`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
