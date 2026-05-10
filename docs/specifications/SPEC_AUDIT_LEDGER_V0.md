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
| `ChainHash` | Ordered integrity evidence threaded through every durable record and anchor. |
| `PolicyVersion` | Non-zero policy version bound to permissioned security, admin, catalog, recovery, backup, restore, HA/DR, and forensic decisions. |
| `AuditRejectionCode` | Stable typed rejection code for invalid append, replay, or export. |

## Invariants

- Audit is append-only.
- Audit record magic and version are validated before replay.
- `AuditRecordHeader` is validated before any payload parse or allocation.
- `AuditLedgerVersion` is explicit, monotonic, and never inferred from payload shape alone.
- Critical audit cannot be globally disabled.
- Records contain principal, certificate, surface, operation, result, and policy version.
- Deletion is detectable.
- ChainHash binds record order and detects gaps, deletion, reorder, and duplicate sequence numbers.
- The first record in a ledger starts from the genesis ChainHash; every later record must carry the previous record ChainHash.
- A non-empty ledger must have persisted chain anchor evidence for first record LSN, last record LSN, record count, and tail ChainHash.
- Permissioned records must carry non-zero `PolicyVersion` and policy digest evidence when policy was consulted.
- A missing policy evaluator, stale policy version, unknown permission, surface mismatch, or denied permission must fail closed before transaction creation.
- Secret values are redacted before persistence.
- Audit ledger evidence is durable forensic and security evidence. It is not a replacement for engine storage truth, WAL truth, manifest truth, or recovery truth.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### Durable audit record format

V0 durable audit records are ASCII field records with explicit version prefix, key/value fields, and checksum suffixes. Binary replacement remains a future version and must keep the same validation semantics.

`AuditLedgerVersion` values are accepted only through an explicit prefix registry. V0 accepts the current `audit-journal-v2` record prefix and may replay legacy `audit-journal-v1` records only through an explicit compatibility path. Implementations must reject unknown prefixes, field-count drift, and native Rust layout persistence.

`AuditRecordHeader` for V0 is the record prefix plus the ordered key/value field envelope below. The header is not optional: an implementation must validate prefix, field count, ASCII shape, record delimiter, and suffix checksums before exposing the payload as audit evidence.

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

The V0 checksum algorithm is `Checksum64 = first 8 bytes of SHA-256(payload) interpreted as big-endian u64, with zero normalized to one`. `ChainChecksum` is `Checksum64("{PreviousChainChecksum:016x}|{Checksum:016x}|{Payload}")`.

### Chain anchor format

Every non-empty durable audit ledger must persist an anchor record containing:

| Anchor field | Required rule |
|---|---|
| AnchorVersion | Explicit supported chain anchor version. |
| FirstRecordLsn | Non-zero first replayable record LSN. |
| LastRecordLsn | Non-zero last replayable record LSN and not lower than `FirstRecordLsn`. |
| RecordCount | Non-zero replayed record count. |
| TailChainHash | Non-zero ChainHash of the final record. |
| AnchorChecksum | Non-zero checksum over the anchor payload. |

Replay must reject a non-empty ledger without an anchor, an anchor without a non-empty ledger, mismatched anchor LSNs, mismatched record count, mismatched tail ChainHash, truncated anchor tail, or anchor checksum mismatch.

### Replay and append outcomes

Replay rejects missing checksum fields, non-ASCII payloads, wrong field counts, checksum mismatch, chain mismatch, duplicate/non-monotonic LSN, missing chain anchor, anchor mismatch, truncated tail, unsupported version, zero checksum evidence, and broken retention compaction proof.

Append for visible critical decisions must be WAL-backed and fail closed if validation, append, or flush cannot prove non-zero `RecordLsn`, `DurableLsn >= RecordLsn`, and non-zero checksum evidence. Security, admin, catalog, HA/DR, backup, restore, and forensic decisions must not become visible without durable audit append evidence. Non-visible or unsupported families must be rejected before append.

Retention compaction must re-chain retained records from the genesis ChainHash, preserve increasing retained LSNs, emit retained checksum evidence, and require archive or forensic-hold proof before dropping expired records.

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

Security and audit rejection traces must distinguish unavailable evidence from absent evidence. For example, pre-policy failures can mark `PolicyVersion` unavailable, but any decision after policy consultation must include non-zero `PolicyVersion` and policy digest evidence.

## Recovery behavior

Recovery replays the durable audit ledger by validating record delimiters, versions, checksums, ChainHash threading, monotonic LSNs, and chain anchor evidence before returning records. Corruption, missing anchors, duplicate LSNs, reordered records, or policy evidence violations keep the replay rejected and must surface typed corruption or validation evidence.

Audit replay can rebuild audit indexes and forensic views, but it does not reconstruct engine storage state. Storage state remains recovered from snapshots, manifests, WAL, and recovery reports.

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
- AuditRecordHeader prefix, field count, delimiter, and checksum tests.
- AuditLedgerVersion unknown-prefix and legacy-prefix tests.
- ChainHash algorithm and golden-vector tests.
- chain anchor present, missing, mismatched, and truncated-tail tests.
- PolicyVersion required-for-permissioned-record tests.
- fail-closed append and flush failure tests.
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
- Reject `permissioned audit record without non-zero PolicyVersion`.
- Reject `non-empty audit ledger without chain anchor`.
- Reject `audit append success without durable LSN and checksum evidence`.
- Reject `AuditRecordHeader inferred from payload only`.
- Reject `unknown AuditLedgerVersion`.
- Reject `unstable audit rejection code`.

## Acceptance summary

Owner: `andromeda-audit` owns the durable audit ledger contract; `andromeda-observe` may map trace events into durable audit families but must not redefine the ledger format.

Evidence: acceptance requires header/version validation tests, ChainHash golden vectors, chain anchor replay tests, fail-closed append/flush tests, retention compaction proof tests, PolicyVersion enforcement tests, and secret-redaction tests tied to this specification.

Reject: acceptance is refused when an implementation accepts unknown `AuditLedgerVersion`, infers `AuditRecordHeader` from payload shape, appends visible critical decisions without durable LSN/checksum proof, replays a non-empty ledger without chain anchor evidence, omits non-zero `PolicyVersion` for permissioned records, or treats audit evidence as engine storage truth.
