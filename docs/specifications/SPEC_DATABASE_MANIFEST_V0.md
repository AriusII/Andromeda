# Specification: DatabaseManifest v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `DatabaseManifest v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define the manifest as recovery entry point.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `DatabaseManifest` | Must be represented as an explicit typed structure or canonical descriptor. |
| `FileListEntry` | Must be represented as an explicit typed structure or canonical descriptor. |
| `SnapshotId` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RequiredWalStartLsn` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PreviousManifestHash` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ManifestHash` | Must be represented as an explicit typed structure or canonical descriptor. |
| `Signature` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ManifestMagic` | Fixed magic value that identifies the manifest before decode. |
| `ManifestVersion` | Explicit manifest format and compatibility version. |

## Invariants

- Manifest is small and verifiable.
- Manifest magic and version are validated before any root is trusted.
- Manifest links to previous manifest.
- Files not listed are ignored.
- Listed invalid files block Online mode.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### DatabaseManifestV0 canonical shape

| Field | Required rule |
|---|---|
| DatabaseId | Non-zero database identity. |
| ManifestVersion | Non-zero monotonic manifest version. |
| SnapshotId | Non-zero snapshot identity. |
| BaseCheckpointLsn | Non-zero checkpoint LSN for the snapshot boundary. |
| RequiredWalStartLsn | Non-zero WAL floor required for recovery. |
| PreviousManifestHash | Non-zero after the first manifest; all-zero is allowed only for genesis if explicitly documented. |
| ManifestCrc | Non-zero integrity check over the canonical manifest shape. |
| ManifestHash | Hash over the canonical shape; required before release promotion. |
| Signature | Required in C5 release mode; P01 may document test mode without signature as non-release evidence. |

Manifest publication is a root switch. Recovery must choose the newest valid manifest whose required WAL range is available; otherwise it falls back to the previous valid root or enters ReadOnly/ForensicOnly according to the RecoveryReport.

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

- manifest hash tests.
- bad magic and unsupported version tests.
- previous manifest fallback tests.
- missing segment tests.
- signature policy tests.
- root switch crash tests.
- required WAL range rejection tests.

## Rejection criteria

- Reject `unsigned manifest in C5 mode`.
- Reject `bad manifest magic`.
- Reject `unsupported manifest version`.
- Reject `file outside manifest`.
- Reject `manifest without WAL range`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
