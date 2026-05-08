# BackupManifest v0 Specification

## Purpose

Define the accepted documentation contract for `BackupManifest v0`, the durable
backup and point-in-time recovery (PITR) manifest evidence used by physical
backup artifact validation and restore preflight.

`BackupManifest v0` binds one cold snapshot to one contiguous WAL archive
range. It is restore evidence. It is not database truth by itself, not a
replacement for WAL replay, and not permission to make a commit visible before
durable WAL.

## Scope

This specification applies to backup metadata owned by `andromeda-storage`:

- logical `BackupManifest` identity and PITR bounds;
- cold snapshot boundary fields;
- WAL archive range fields;
- file-backed backup artifact manifest bytes;
- compatibility evidence for manifest, physical plan, storage, and WAL formats;
- restore preflight evidence derived from the manifest.

The current Rust artifact writer emits artifact manifest format version `3`.
This document is named `v0` because it is the first accepted specification for
the logical backup manifest contract.

## Non-goals

This specification does not:

- introduce ad hoc SQL, dynamic command text, gRPC, or untyped restore payloads;
- expose backup, restore, PITR, Administration, or HA/DR behavior through the
  Application surface;
- serialize Rust structs directly to disk or network;
- define the WAL record frame format, page format, or cold segment layout;
- make RAM, temp files, GPU output, benchmark output, or audit events database
  truth;
- define encryption, key management, compression, remote object storage, or
  multi-node backup scheduling;
- authorize restore without checksum, digest, WAL chain, PITR target, and
  compatibility validation.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `crates/andromeda-storage/src/backup/plan.rs` for `BackupManifest`,
  `PitrTarget`, PITR audit records, and manifest validation.
- `crates/andromeda-storage/src/backup/artifact_store.rs` for file-backed
  artifact directory validation and durable WAL archive evidence.
- `crates/andromeda-storage/src/backup/artifact_store/manifest_format.rs` for
  artifact manifest header, payload layout, versions, checksums, and
  little-endian encoding.
- `crates/andromeda-storage/src/backup/artifacts.rs` for cold snapshot, WAL
  segment, compatibility, resource, incomplete transaction, and audit evidence.
- `crates/andromeda-storage/src/restore_orchestration/preflight.rs` for restore
  preflight behavior.
- `documentations/specs/WalRecord_v0.md` and
  `documentations/specs/RecoveryReport_v0.md` for WAL and recovery evidence
  boundaries.

## Procedure

### Ownership

`andromeda-storage` owns the backup manifest domain model, artifact manifest
codec, and restore preflight interpretation. Callers may pass typed values into
these APIs, but persistent bytes must be produced and consumed only through the
explicit backup artifact manifest codec.

The Rust `BackupManifest`, `BackupColdSnapshotArtifact`, and
`BackupWalSegmentArtifact` structs are domain models. Their native memory layout
is never a disk or network format.

### Logical manifest fields

`BackupManifest v0` contains these logical fields:

| Field | Type | Rule |
| --- | --- | --- |
| `backup_id` | `BackupId` | Must be nonzero and uniquely identify one completed backup artifact directory. |
| `database_id` | `u64` | Must be nonzero. |
| `created_epoch` | `u64` | Must be nonzero and matches the cold snapshot artifact manifest version. |
| `snapshot.snapshot_id` | `u64` | Must identify the cold snapshot boundary. |
| `snapshot.snapshot_descriptor_hash` | `[u8; 32]` | Must be nonzero in accepted artifacts and match the cold snapshot artifact. |
| `snapshot.base_checkpoint_lsn` | `Lsn` | Earliest restorable target. A PITR target equal to this LSN is snapshot-only and requires no WAL replay. |
| `snapshot.required_wal_start_lsn` | `Lsn` | First WAL LSN required to replay beyond the snapshot base. |
| `wal_archive.start` | `Lsn` | First LSN present in the archived WAL range. It must be less than or equal to `required_wal_start_lsn`. |
| `wal_archive.end_inclusive` | `Lsn` | Latest restorable PITR target. It must not precede the snapshot base checkpoint. |
| `manifest_crc` | `u32` | Must be nonzero and match the cold snapshot artifact evidence. |

The effective PITR window is:

```text
earliest = snapshot.base_checkpoint_lsn
latest = wal_archive.end_inclusive
```

Targets below `snapshot.base_checkpoint_lsn`, between the snapshot base and
`snapshot.required_wal_start_lsn`, or after `wal_archive.end_inclusive` must be
rejected. The only accepted target before `required_wal_start_lsn` is exactly
`snapshot.base_checkpoint_lsn`.

### Artifact directory

The file-backed artifact store writes one immutable directory per backup id:

```text
backup-{backup_id:016x}/
  backup.manifest
  snapshot.bin
  wal/
    segment-{sequence_index:06}-{segment_id:016x}.wal
```

A completed backup artifact directory must not be overwritten. A second write
for the same `backup_id` must fail before mutating existing artifact files.

Restore preflight must validate:

- manifest file magic, version, payload length, and payload checksum;
- cold snapshot artifact digest, CRC64, byte length, manifest version, snapshot
  id, descriptor hash, and manifest CRC;
- every WAL segment artifact digest, CRC64, byte length, LSN range, predecessor
  link, record count, and WAL format version;
- aggregate WAL archive evidence;
- PITR target bounds;
- replay segment planning from archive start to the target LSN;
- restore evidence checksum binding the selected PITR target to durable
  artifact evidence.

### Encoding policy

All multi-byte integer fields in the persisted file-backed artifact manifest
use explicit little-endian encoding. No native Rust struct layout is persisted.

The current artifact manifest file wrapper is:

| Offset | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 29 | `magic` | ASCII `ANDROMEDA-BACKUP-ARTIFACT-V1\n`. |
| 29 | 2 | `format_version` | Little-endian `u16`. Current writer emits `3`; readers accept `1`, `2`, and `3`. |
| 31 | 8 | `payload_len` | Little-endian `u64`; exact payload byte count. |
| 39 | 32 | `payload_sha256` | SHA-256 of the payload bytes. |
| 71 | `payload_len` | `payload` | Versioned manifest payload. |

The decoder must reject truncated headers, magic mismatch, unsupported
versions, payload length mismatch, payload checksum mismatch, and trailing
bytes.

### Current v3 payload layout

The current writer emits payload format version `3`. All offsets in this table
are relative to the payload start.

| Offset | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `backup_id` | Little-endian `u64`, nonzero. |
| 8 | 8 | `database_id` | Little-endian `u64`, nonzero. |
| 16 | 8 | `created_epoch` | Little-endian `u64`, nonzero. |
| 24 | 8 | `snapshot_id` | Little-endian `u64`. |
| 32 | 32 | `snapshot_descriptor_hash` | Exact 32-byte descriptor hash. |
| 64 | 8 | `base_checkpoint_lsn` | Little-endian `u64`. |
| 72 | 8 | `required_wal_start_lsn` | Little-endian `u64`. |
| 80 | 8 | `wal_archive_start_lsn` | Little-endian `u64`. |
| 88 | 8 | `wal_archive_end_inclusive_lsn` | Little-endian `u64`. |
| 96 | 4 | `manifest_crc` | Little-endian `u32`, nonzero. |
| 100 | 8 | `source_checkpoint_lsn` | Must equal `base_checkpoint_lsn`. |
| 108 | 8 | `wal_evidence_start_lsn` | Must equal `wal_archive_start_lsn`. |
| 116 | 8 | `wal_evidence_end_lsn` | Must equal `wal_archive_end_inclusive_lsn`. |
| 124 | 8 | `wal_evidence_segment_count` | Little-endian `u64`; bounded and nonzero. |
| 132 | 8 | `wal_evidence_total_bytes` | Little-endian `u64`; sum of WAL artifact byte lengths. |
| 140 | 32 | `wal_evidence_archive_sha256` | Aggregate digest over WAL segment artifact evidence. |
| 172 | 8 | `cold_snapshot_database_id` | Must match `database_id`. |
| 180 | 8 | `cold_snapshot_manifest_version` | Must match `created_epoch`. |
| 188 | 8 | `cold_snapshot_snapshot_id` | Must match `snapshot_id`. |
| 196 | 32 | `cold_snapshot_descriptor_hash` | Must match `snapshot_descriptor_hash`. |
| 228 | 4 | `cold_snapshot_manifest_crc` | Must match `manifest_crc`. |
| 232 | 32 | `cold_snapshot_sha256` | Nonzero artifact digest. |
| 264 | 8 | `cold_snapshot_crc64` | Nonzero artifact checksum. |
| 272 | 8 | `cold_snapshot_byte_len` | Nonzero byte length. |
| 280 | 8 | `wal_segment_count` | Must match `wal_evidence_segment_count`. |
| 288 | `91 * wal_segment_count` | `wal_segment_entries` | Ordered WAL segment artifact entries. |
| `288 + 91 * wal_segment_count` | 8 | `compatibility_evidence` | Four little-endian `u16` fields. |

Each WAL segment entry is exactly 91 bytes:

| Entry offset | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `segment_id` | Little-endian `u64`, nonzero and unique in the archive. |
| 8 | 8 | `first_lsn` | Little-endian `u64`, nonzero. |
| 16 | 8 | `last_lsn` | Little-endian `u64`, must be greater than or equal to `first_lsn`. |
| 24 | 1 | `base_previous_lsn_tag` | `0` for absent, `1` for present. |
| 25 | 8 | `base_previous_lsn_value` | Zero when absent; nonzero and exactly preceding `first_lsn` when present. |
| 33 | 8 | `record_count` | Little-endian `u64`, nonzero. |
| 41 | 2 | `wal_format_version` | Little-endian `u16`, must match the supported WAL format. |
| 43 | 32 | `artifact_sha256` | Nonzero artifact digest. |
| 75 | 8 | `artifact_crc64` | Nonzero artifact checksum. |
| 83 | 8 | `artifact_byte_len` | Nonzero byte length. |

The compatibility evidence trailer contains:

| Trailer offset | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 2 | `manifest_format_version` | Must equal the wrapper `format_version`. |
| 2 | 2 | `physical_plan_version` | Must equal the supported physical plan version. |
| 4 | 2 | `storage_format_version` | Must equal the supported storage format version. |
| 6 | 2 | `wal_format_version` | Must equal the supported WAL format version. |

Legacy format versions `1` and `2` may be read for compatibility. Version `1`
reconstructs aggregate WAL archive digest from segment evidence. Versions `1`
and `2` reconstruct compatibility evidence and mark it as not recorded in the
manifest. The current writer must emit version `3` with compatibility evidence
recorded in the manifest.

### WAL archive validation

The WAL archive segment list must be contiguous:

1. The first segment starts at `wal_archive.start`.
2. Each segment's `base_previous_lsn` equals the prior segment's `last_lsn`,
   except the first segment, which must have no predecessor.
3. Each next segment starts at `prior.last_lsn + 1`.
4. Segment ids do not repeat.
5. The last segment ends at `wal_archive.end_inclusive`.

Restore planning may stop at the segment containing the PITR target, but
artifact preflight must validate the full durable manifest and full WAL archive
evidence before returning a restore evidence checksum.

### Retention and immutability

Backup artifacts are retention evidence. They must remain immutable while they
anchor any recovery or PITR window.

WAL reclaimability must consider these boundaries from one deterministic
snapshot:

- crash recovery floor;
- active snapshot visibility floor;
- required replica shipping floor;
- PITR retention LSN.

A WAL segment with `segment_end_lsn >= pitr_retention_lsn` remains inside the
PITR retention window and must not be reclaimed. If a backup artifact directory
exists for a backup id, attempts to rewrite it must fail before changing the
existing manifest, snapshot, or WAL files.

### Recovery evidence

Restore preflight produces a bounded evidence object, not restored database
truth. The preflight evidence must include:

- backup id and artifact root;
- manifest format version;
- validation policy;
- selected PITR target LSN;
- source checkpoint LSN;
- manifest and snapshot artifact digests;
- aggregate WAL archive evidence;
- restore evidence checksum;
- replay segment count.

The restore evidence checksum must bind the selected PITR target, manifest
format version, manifest digest, snapshot digest, and aggregate WAL archive
evidence. A different PITR target over the same artifact set must produce
different restore evidence.

Forensic restore orchestration must use full validation and bind its audit
trace to durable preflight evidence before any recovery decision can be
accepted.

## Validation

Documentation acceptance checks:

- The spec states that `BackupManifest v0` is restore evidence, not database
  truth.
- The spec states that persistent bytes use explicit little-endian codecs and
  never Rust native struct layout.
- The spec documents the current artifact wrapper header and v3 payload layout.
- The spec preserves WAL-before-visible-commit and cold snapshot plus durable
  WAL truth boundaries.
- The spec requires PITR target bounds, WAL archive contiguity, artifact digest
  validation, compatibility evidence, immutable artifact directories, and
  restore evidence checksum binding.
- The spec does not introduce SQL, gRPC, runtime JSON defaults, or Application
  surface administration.

Executable validation to keep with this spec:

```powershell
cargo test -p andromeda-storage --test backup_execution_plan
cargo test -p andromeda-storage --test restore_contract
cargo test -p andromeda-storage --test wal_shipping_reclaimability_contract
cargo test -p andromeda-storage --test recovery_contract
```

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Restore accepts a target between the snapshot base and required WAL start. | PITR validation treated the WAL gap as restorable. | Reject the target unless it exactly equals the snapshot base checkpoint. |
| Restore uses the manifest but skips WAL archive evidence validation. | Preflight trusted logical metadata without durable artifact proof. | Validate manifest bytes, snapshot artifact, every WAL segment, aggregate WAL digest, and replay plan. |
| A second backup write replaces an existing backup id. | Artifact directory immutability was bypassed. | Fail before overwrite and preserve the original artifact directory. |
| WAL GC removes a segment inside the PITR window. | PITR retention boundary was omitted from reclaimability evidence. | Block reclaim while `segment_end_lsn >= pitr_retention_lsn`. |
| Manifest documentation relies on Rust struct field order. | Native layout was confused with durable bytes. | Use the explicit byte tables in this spec and the manifest codec source. |
| Legacy manifests are treated as current evidence. | v1/v2 compatibility reconstruction was not distinguished from v3 persisted evidence. | Mark compatibility evidence as reconstructed for legacy reads and require v3 for new writes. |
| Recovery audit checksum matches the manifest but not artifact preflight. | The audit bound only logical manifest evidence. | Use preflight evidence checksum for restore paths that validated durable artifact files. |

## References

- `crates/andromeda-storage/src/backup.rs`
- `crates/andromeda-storage/src/backup/plan.rs`
- `crates/andromeda-storage/src/backup/artifacts.rs`
- `crates/andromeda-storage/src/backup/artifact_store.rs`
- `crates/andromeda-storage/src/backup/artifact_store/manifest_format.rs`
- `crates/andromeda-storage/src/backup/artifact_store/payload_cursor.rs`
- `crates/andromeda-storage/src/restore_orchestration/preflight.rs`
- `crates/andromeda-storage/src/restore_orchestration/replay_plan.rs`
- `crates/andromeda-storage/tests/backup_execution_plan.rs`
- `crates/andromeda-storage/tests/restore_contract.rs`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
