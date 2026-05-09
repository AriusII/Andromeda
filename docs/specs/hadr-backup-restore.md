# HA/DR, Backup, And Restore

## Purpose

This spec defines HA/DR quorum, fencing, WAL shipping, promotion, backup
manifest, PITR, restore validation, and retention contracts. It preserves
single-primary v0 semantics and the rule that no replicated or restored state
is authoritative without durable WAL and validated storage evidence.

## HA/DR role model

| Role | Meaning | Write authority |
| --- | --- | --- |
| `Primary` | Node currently authorized by the active fencing token. | Allowed only under the matching active token and WAL gates. |
| `Replica` | Read-only follower that receives and validates WAL. | Not allowed. |
| `Candidate` | Replica staging promotion for a proposed epoch. | Not allowed until promotion commits and a token is active. |

`HadrEpoch` is durable role and fencing epoch. Promotion strictly advances it.
Runtime membership epoch and cluster manifest version are separate evidence
classes and must not be conflated.

## Quorum and fencing

| Mode | Minimum replica ACKs before visibility | Quorum-loss behavior |
| --- | ---: | --- |
| `Asynchronous` | 0 | Quorum loss does not block by itself, but degraded evidence must be emitted. |
| `QuorumEnforced` | Active majority quorum | Quorum loss blocks new write visibility. |

Fencing token enforcement is required before every primary-side visible
mutation. Two nodes claiming primary at the same epoch is a split-brain
failure: writes must be blocked, evidence preserved, and operator recovery
required before accepting either node.

Failure evidence for replica disconnect, checksum mismatch, LSN gap, and
unknown failures must feed quorum and fencing decisions. In quorum-enforced
mode, unknown safety must fail closed unless explicit degraded policy permits
the operation.

## Promotion

Promotion requires authenticated cluster identity, durable membership snapshot,
unique voter roster, active token evidence, proposed higher epoch, candidate
safe LSN, validated WAL shipping ACKs, and a durable audit record.

A no-loss promotion may succeed only when the candidate safe LSN covers the
required safe point for granting voters. Promotion must reject duplicate voters,
stale or lower epochs, active tokens at the same or higher epoch, and candidate
safe LSNs below granting voter evidence.

HA/DR manifest publication must carry quorum-granted update evidence and must
not bypass durable WAL, storage, recovery, or audit gates.

## WAL shipping

WAL shipping is single-primary in v0. It ships durable WAL evidence from the
active primary to replicas; it is not database truth by itself.

| Role | Allowed behavior |
| --- | --- |
| `Primary` | Reads durable WAL ranges and ships them to replicas. |
| `Replica` | Receives, validates, durably appends, and ACKs contiguous WAL ranges. |

A shipped range is valid only when it is at or below `primary_durable_lsn`,
contains contiguous WAL records, carries exact start and end LSNs, record count
and checksum evidence, and preserves previous-LSN chain expectations.

Replica ACKs are promotion and retention evidence only after the replica has
validated and durably appended the range. ACKs from receive memory are rejected.
Retention may reclaim WAL only above the minimum required replica safe LSN and
outside PITR, backup, restore, and forensic holds.

## Backup manifest

`BackupManifest v0` binds one cold snapshot to one contiguous WAL archive. It
is not a replacement for WAL replay and does not permit visible commit before
durable WAL.

| Field | Rule |
| --- | --- |
| `backup_id` | Nonzero and unique for one completed artifact directory. |
| `database_id` | Nonzero. |
| `snapshot.snapshot_id` | Cold snapshot boundary. |
| `snapshot.snapshot_descriptor_hash` | Nonzero and must match the snapshot artifact. |
| `snapshot.base_checkpoint_lsn` | Earliest restorable target; equal target is snapshot-only. |
| `snapshot.required_wal_start_lsn` | First WAL LSN required to replay beyond the snapshot base. |
| `wal_archive.start` | Must be less than or equal to `required_wal_start_lsn`. |
| `wal_archive.end_inclusive` | Latest restorable PITR target and must not precede the snapshot base. |
| `manifest_crc` | Nonzero and matches cold snapshot artifact evidence. |

The artifact wrapper magic is `ANDROMEDA-BACKUP-ARTIFACT-V1\n`. Current writers
emit format version `3`; readers may reconstruct compatibility evidence for
versions `1`, `2`, and `3`. New writes must preserve v3 evidence for cold
snapshot artifact digest, CRC64, byte length, WAL segment entries, aggregate
WAL archive digest, and compatibility versions.

Artifact directories are immutable. A second write for an existing backup id
must fail before overwrite.

## WAL archive and PITR

WAL archive segments must be contiguous. Each segment entry records segment id,
first and last LSN, predecessor evidence, record count, WAL format version,
artifact SHA-256, artifact CRC64, and byte length.

A PITR target equal to `base_checkpoint_lsn` requires no WAL replay. A target
between `base_checkpoint_lsn` and `required_wal_start_lsn` is rejected unless it
is exactly the base checkpoint. Targets above the base require a validated WAL
archive from `required_wal_start_lsn` through the target.

Restore preflight must validate manifest bytes, snapshot artifact, every WAL
segment, aggregate WAL archive digest, compatibility evidence, PITR bounds,
and replay plan before accepting the restore.

## Retention

WAL reclaimability must consider active manifest roots, backup manifests, PITR
retention LSN, replica safe LSNs, restore validation windows, and forensic
holds. A WAL segment with `segment_end_lsn` greater than or equal to the PITR
retention LSN remains inside the PITR window.

Cleanup must not remove the previous valid snapshot, manifest, segment index,
or WAL archive evidence while recovery, backup, replica, restore, or forensic
policy can still require it.

## Validation gates

- Quorum tests must prove write blocking on quorum loss in quorum-enforced
  mode.
- Fencing tests must reject old-primary writes after promotion.
- WAL shipping tests must reject non-durable shipments, gaps, checksum
  mismatch, and ACK before durable append.
- Promotion tests must reject duplicate voters and insufficient candidate safe
  LSN.
- Backup and restore tests must validate artifact immutability, archive
  contiguity, PITR gaps, aggregate digest, and replay plan.
