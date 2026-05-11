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
- Root switch is accepted only after WAL evidence covers the manifest checkpoint.
- The active manifest is selected by validity first, then highest ManifestVersion.
- Native Rust layout is not a manifest format.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### DatabaseManifestV0 canonical shape

DatabaseManifestV0 is a canonical little-endian byte format, not a serialized Rust struct. The manifest has a 192-byte header, zero or more fixed 128-byte file entries ordered by `(FileKindTag, FileId)`, optional extension bytes, optional signature bytes, and a 96-byte trailer. All reserved bytes must be zero.

| Header offset | Size | Field | Required rule |
|---:|---:|---|---|
| 0 | 8 | ManifestMagic | Must be `ANDMFST0`. |
| 8 | 2 | FormatMajor | Must be `1`. |
| 10 | 2 | FormatMinor | Must be `0`. |
| 12 | 2 | ByteOrder | Must be `0x0102`. |
| 14 | 2 | HeaderLength | Must be `192`. |
| 16 | 8 | TotalLength | Must equal header + entries + extension + signature + trailer. |
| 24 | 4 | HeaderCrc32 | CRC32/ISO-HDLC with this field zeroed. |
| 28 | 4 | Flags | Only known genesis, signed, forensic-hold, and test-unsigned bits are valid. |
| 32 | 8 | DatabaseId | Non-zero database identity. |
| 40 | 8 | ManifestVersion | Non-zero monotonic manifest version. |
| 48 | 8 | SnapshotId | Non-zero snapshot identity. |
| 56 | 8 | BaseCheckpointLsn | Non-zero checkpoint LSN for the snapshot boundary except genesis. |
| 64 | 8 | RequiredWalStartLsn | Non-zero WAL floor required for recovery except genesis. |
| 72 | 8 | LastManifestLsn | WAL LSN of the manifest publication record when available; zero only for documented genesis. |
| 80 | 32 | PreviousManifestHash | Non-zero after the first manifest; all-zero is allowed only for genesis with the genesis flag. |
| 112 | 32 | StorageFormatFingerprintHash | Deterministic hash of durable format fingerprints. |
| 144 | 8 | FileEntryOffset | Must be `192`. |
| 152 | 8 | FileEntryCount | Must fit the entry table bound. |
| 160 | 4 | FileEntryLength | Must be `128`. |
| 164 | 4 | ManifestCrc | Non-zero CRC32/ISO-HDLC over the canonical manifest body with hash/signature fields zeroed. |
| 168 | 8 | ExtensionOffset | Must follow the file entry table. |
| 176 | 8 | ExtensionLength | Must be bounded by implementation policy and covered by ManifestHash. |
| 184 | 8 | Reserved | Must be zero. |

| File entry offset | Size | Field | Required rule |
|---:|---:|---|---|
| 0 | 2 | FileKindTag | `1` page segment, `2` SegmentIndex, `3` WAL segment, `4` audit ledger, `5` backup metadata. |
| 2 | 2 | EntryFormatVersion | Must be `1`. |
| 4 | 4 | EntryFlags | Only known immutable, required-for-online, and optional-for-readonly bits are valid. |
| 8 | 8 | FileId | Non-zero stable identity. |
| 16 | 8 | SegmentIdOrZero | Non-zero for segment-backed files. |
| 24 | 8 | ByteOffset | Canonical byte offset inside the artifact. |
| 32 | 8 | ByteLength | Non-zero and must not overflow file bounds. |
| 40 | 8 | MinPageId | Zero when not page-addressable. |
| 48 | 8 | MaxPageId | Must be >= MinPageId when MinPageId is non-zero. |
| 56 | 8 | MinPageLsn | Zero only when not page-addressable. |
| 64 | 8 | MaxPageLsn | Must be >= MinPageLsn when MinPageLsn is non-zero. |
| 72 | 8 | PayloadCrc64 | Non-zero integrity evidence for the referenced bytes. |
| 80 | 32 | PayloadSha256 | Non-zero SHA-256 over the referenced bytes. |
| 112 | 8 | RequiredWalStartLsn | WAL floor needed by this file; zero only when not WAL-covered. |
| 120 | 4 | EntryCrc32 | CRC32/ISO-HDLC over the 128-byte entry with this field zeroed. |
| 124 | 4 | Reserved | Must be zero. |

| Trailer offset | Size | Field | Required rule |
|---:|---:|---|---|
| 0 | 32 | ManifestHash | SHA-256 over the canonical manifest bytes with ManifestHash and Signature bytes zeroed. |
| 32 | 32 | FileTableSha256 | SHA-256 over the encoded file entry table. |
| 64 | 8 | FileTableCrc64 | CRC64/ECMA over the encoded file entry table. |
| 72 | 8 | SignatureOffset | Zero only when unsigned test mode is explicitly flagged. |
| 80 | 4 | SignatureLength | Zero only when unsigned test mode is explicitly flagged. |
| 84 | 4 | TrailerCrc32 | CRC32/ISO-HDLC over the trailer with this field zeroed. |
| 88 | 8 | Reserved | Must be zero. |

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

### Hash, CRC, and signature rules

`HeaderCrc32`, `EntryCrc32`, and `TrailerCrc32` use CRC32/ISO-HDLC with reflected polynomial `0xEDB88320`, initial value `0xFFFF_FFFF`, final xor `0xFFFF_FFFF`, and the target CRC field zeroed.

`FileTableCrc64` uses CRC64/ECMA with polynomial `0x42F0E1EBA9EA3693`, initial value `0`, no final xor, and zero normalized to `1`.

`PayloadCrc64` field in file entries uses the CRC64/ECMA polynomial `0x42F0E1EBA9EA3693` (a.k.a. ISO 3309 CRC-64); the canonical reference implementation is `crc::Crc::<u64>::new(&crc::CRC_64_ECMA_182)`.

`ManifestHash` is SHA-256 over the canonical manifest bytes after setting ManifestHash to zero and setting signature bytes to zero. `PreviousManifestHash` must equal the prior accepted ManifestHash except for genesis.

`StorageFormatFingerprintHash` is the deterministic manifest fingerprint hash over `(DatabaseId, ManifestVersion, SnapshotId, ordered format kind/version records)`. V0 kind tags are Page `1`, HeapPage `2`, BTreeKey `3`, BTreeNode `4`, WalRecord `5`, WalPayload `6`, Manifest `7`, Segment `8`, and Checkpoint `9`.

`Signature` signs ManifestHash and root-switch metadata. C5 release mode rejects unsigned manifests. P01 unsigned test mode is allowed only with the test-unsigned flag and must not be retained as release evidence.

### Root switch protocol

Manifest publication is an atomic root switch:

1. Stage manifest bytes under a non-root name and fsync the manifest bytes.
2. Validate every listed file hash, CRC, length, LSN range, and required-for-online flag.
3. Require `WalCheckpointLsn <= DurableWalLsn` and `BaseCheckpointLsn <= WalCheckpointLsn`.
4. Reject bootstrap if ManifestVersion or checkpoint evidence conflicts with nonzero WAL evidence.
5. Publish the root pointer with the candidate ManifestHash and ManifestVersion.
6. Fsync the root pointer directory or platform equivalent before acknowledging publication.
7. Retain the previous root pointer until the new root has survived recovery validation.

Recovery must examine candidate roots without trusting file names alone. It accepts the newest manifest that passes magic, version, length, CRC, hash, signature policy, previous-hash chain, file-list, SegmentIndex, and WAL-range validation. If the newest root is partial or invalid, recovery falls back to the previous valid root. If no root can be validated, startup enters ReadOnly or ForensicOnly according to RecoveryReport.

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

Recovery treats the manifest as the durable entry point. It validates the root pointer, DatabaseManifestV0 bytes, file entries, SegmentIndex binding, and required WAL range before Online mode. Files not listed in the accepted manifest are ignored. Listed files with invalid required-for-online evidence block Online mode. RecoveryReport must record the selected ManifestVersion, ManifestHash, previous-root fallback if used, RequiredWalStartLsn, and any rejected candidate root.

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
- canonical manifest byte layout golden vector tests.
- file entry ordering and EntryCrc32 tests.
- ManifestHash and PreviousManifestHash chain tests.
- unsigned test-mode versus C5 signature policy tests.
- root pointer torn-write and previous-root fallback crash tests.
- manifest-listed SegmentIndex mismatch tests.
- malformed corpus coverage under `tests/fuzzing/corpus/manifest_decode` and `tests/fuzzing/corpus/manifest_boundary`.

## Rejection criteria

- Reject `unsigned manifest in C5 mode`.
- Reject `bad manifest magic`.
- Reject `unsupported manifest version`.
- Reject `file outside manifest`.
- Reject `manifest without WAL range`.
- Reject `manifest native Rust layout`.
- Reject `manifest hash mismatch`.
- Reject `manifest CRC mismatch`.
- Reject `previous manifest hash mismatch`.
- Reject `partial root switch without previous valid root`.
- Reject `listed required file with invalid hash or CRC`.
- Reject `SegmentIndex not bound to manifest`.

## Acceptance summary

Owner: Personne 09 owns DatabaseManifest V0 with the `andromeda-manifest`, `andromeda-segment`, `andromeda-wal`, and `andromeda-recovery` owner crates.

Evidence: acceptance requires canonical manifest byte layout golden vectors, ManifestHash and PreviousManifestHash chain tests, root switch crash tests, required WAL range rejection tests, signature policy tests, manifest-listed SegmentIndex binding tests, and malformed corpus coverage under `tests/fuzzing/corpus/manifest_decode` and `tests/fuzzing/corpus/manifest_boundary`.

Reject: reviewers must reject implementations that persist native Rust layout, skip ManifestMagic or ManifestVersion validation, accept a bad ManifestHash, ManifestCrc, PreviousManifestHash, file-entry hash or CRC, allow an unsigned C5 manifest, publish a root switch before WAL evidence covers the checkpoint, or enter Online mode with an invalid required manifest file.
