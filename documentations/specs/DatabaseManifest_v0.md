# DatabaseManifest v0 Specification

## Purpose

Define the accepted documentation contract for `DatabaseManifest v0`, the
durable recovery root that binds a database snapshot, storage format
fingerprints, cold segment indexes, previous manifest identity, and the WAL
range required to reconstruct database truth.

`DatabaseManifest v0` is persisted storage metadata. It is not a Rust native
struct layout, not a cache image, and not permission to make a transaction
visible before durable WAL evidence exists.

## Scope

This specification applies to the physical database manifest artifact used by
startup, recovery, cold snapshot publication, manifest switching, backup
preflight, restore preflight, and forensic inspection.

It covers:

- explicit little-endian manifest file fields;
- manifest versioning and compatibility rules;
- storage format fingerprint records;
- cold `SegmentIndex v0` root references;
- checksum, hash, signature, and validation expectations;
- manifest switch and recovery behavior;
- fuzz, golden-vector, and crash validation gates.

## Current Implementation Status

The Rust workspace currently has a `DatabaseManifest` domain model with these
validated fields:

- `database_id`;
- `manifest_version`;
- `snapshot_id`;
- `base_checkpoint_lsn`;
- `required_wal_start_lsn`;
- `previous_manifest_hash`;
- `manifest_crc`.

The workspace also has `StorageFormatManifest`, snapshot publication contracts,
and cold segment publication guards. Those Rust types are domain models and
validation evidence. They are not the durable manifest byte layout.

This specification defines the required durable byte contract that storage,
recovery, backup, restore, and fuzz work must converge on before manifest
publication is treated as a release compatibility promise.

## Non-goals

This specification does not:

- introduce ad hoc SQL, gRPC, runtime JSON defaults, or untyped Procedure
  bypasses;
- serialize Rust native structs, enums, padding, `Vec`, `Option`, or pointer
  layouts to disk;
- define page, heap, B-Tree, WAL frame, audit ledger, backup artifact, or
  Procedure contract payload bytes;
- make RAM, temp files, HotStore state, GPU output, benchmark output, audit
  records, or trace records database truth;
- authorize manifest publication without durable WAL and fsync evidence;
- authorize recovery replay of unknown storage formats;
- expose manifest switching, recovery, backup, restore, or forensic behavior
  through the Application surface.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for Andromeda storage, WAL, recovery, and binary-codec
  invariants.
- `documentations/ROADMAP_IMPLEMENTATION_2026.md` for durable storage V0,
  cold snapshot publication, and recovery priorities.
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` for manifest,
  snapshot, WAL, and recovery truth concepts.
- `documentations/governance/decisions/DEC-032-storage-format-gate.md` for
  pre-redo storage format validation and pre-persistence candidate status.
- `documentations/specs/PageHeader_PageTrailer_v0.md` for page format identity
  and torn-write validation.
- `documentations/specs/WalRecord_v0.md` for durable WAL and `ManifestSwitch`
  record evidence.
- `documentations/specs/SegmentIndex_v0.md` for cold segment index bytes.
- `crates/andromeda-storage/src/manifest.rs` for current manifest domain
  validation.
- `crates/andromeda-storage/src/manifest/format.rs` for current storage format
  fingerprint evidence.
- `crates/andromeda-storage/src/manifest/snapshot.rs` for snapshot publication
  validation.
- `crates/andromeda-storage/src/segment.rs` and `crates/andromeda-storage/src/cold_store.rs`
  for segment identity and ColdStore immutability evidence.

## Procedure

### Ownership

`andromeda-storage` owns the durable database manifest format and explicit
manifest codec.

Other crates may consume manifest identity, recovery floors, storage format
fingerprints, and publication evidence through typed APIs. They must not define
their own manifest byte layout or persist the Rust `DatabaseManifest` memory
layout.

### Encoding policy

All multi-byte integer fields in `DatabaseManifest v0` use little-endian
encoding. Fixed digests and signatures are byte arrays and are copied exactly
as bytes.

The manifest codec must write and read fields by explicit offsets. It must not
derive bytes from Rust native struct layout, `bincode`, `serde` defaults,
debug output, pointer layout, platform alignment, or host endianness.

The manifest file has this top-level shape:

```text
DatabaseManifestFileV0 =
    ManifestHeaderV0
    FormatFingerprintEntryV0[format_fingerprint_count]
    SegmentIndexRootEntryV0[segment_index_root_count]
    ExtensionBytes[extension_len]
    SignatureBytes[signature_len]
    ManifestTrailerV0
```

All offset and length fields are absolute byte offsets from the start of the
manifest file. Readers must use checked arithmetic before converting lengths to
allocation sizes.

### Constants

| Constant | Value | Rule |
| --- | ---: | --- |
| `DATABASE_MANIFEST_MAGIC` | `41 4e 44 4d 41 4e 30 00` | Literal bytes `ANDMAN0\0`. |
| `DATABASE_MANIFEST_FORMAT_MAJOR` | `1` | Initial durable manifest major version. |
| `DATABASE_MANIFEST_FORMAT_MINOR` | `0` | Initial durable manifest minor version. |
| `DATABASE_MANIFEST_BYTE_ORDER` | `0x0102` | Required little-endian byte-order marker. |
| `DATABASE_MANIFEST_HEADER_LEN` | `320` | Fixed header length. |
| `DATABASE_MANIFEST_TRAILER_LEN` | `96` | Fixed trailer length. |
| `FORMAT_FINGERPRINT_ENTRY_LEN` | `16` | Fixed storage format fingerprint entry length. |
| `SEGMENT_INDEX_ROOT_ENTRY_LEN` | `96` | Fixed segment index root reference length. |
| `DATABASE_MANIFEST_MAX_FINGERPRINTS` | `256` | Decode and validation bound. |
| `DATABASE_MANIFEST_MAX_SEGMENT_INDEX_ROOTS` | `4096` | Decode and validation bound. |
| `DATABASE_MANIFEST_MAX_EXTENSION_BYTES` | `1 MiB` | Extension decode bound. |
| `DATABASE_MANIFEST_MAX_SIGNATURE_BYTES` | `4096` | Signature decode bound. |

The file name uses `v0` because this is the first accepted documentation
contract. The encoded durable format version uses major `1`, minor `0` to
match the existing `FormatVersion::V1_0` convention.

### Checksum and hash algorithms

`DatabaseManifest v0` uses these integrity algorithms:

| Evidence | Algorithm | Normalization |
| --- | --- | --- |
| `header_crc32`, entry CRC fields, `trailer_crc32` | CRC-32/ISO-HDLC, reflected polynomial `0xedb88320`, initial value `0xffff_ffff`, final xor `0xffff_ffff` | If the computed value is zero, encode `1` and require readers to apply the same normalization. |
| `payload_crc64` | CRC-64/ECMA-182, polynomial `0x42f0_e1eb_a9ea_3693`, initial value `0`, final xor `0` | If the computed value is zero, encode `1` and require readers to apply the same normalization. |
| SHA-256 fields | SHA-256 over the canonical byte ranges named by each field | Required hashes must not be all zero. |

Changing any checksum, hash, normalization, or canonical input rule is an
incompatible manifest format change unless a future extension explicitly
version-gates both old and new algorithms.

### Manifest header layout

The encoded `ManifestHeaderV0` is fixed at 320 bytes.

| Offset | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `magic` | Must equal `DATABASE_MANIFEST_MAGIC`. |
| 8 | 2 | `format_major` | Must equal `1` for this spec. |
| 10 | 2 | `format_minor` | Must equal `0` unless an accepted compatibility rule allows a lower required feature set. |
| 12 | 2 | `byte_order` | Must equal `0x0102`. |
| 14 | 2 | `header_len` | Must equal `320`. |
| 16 | 8 | `total_len` | Total manifest file length including trailer. |
| 24 | 4 | `header_crc32` | CRC-32 over the 320-byte header with bytes 24 through 27 zeroed. |
| 28 | 4 | `flags` | Only known bits are valid. |
| 32 | 8 | `database_id` | Must be nonzero. |
| 40 | 8 | `manifest_version` | Must be nonzero and monotonically increase within one manifest chain. |
| 48 | 8 | `manifest_epoch` | Publication epoch; must be nonzero. |
| 56 | 8 | `snapshot_id` | Must be nonzero and match every segment index root entry. |
| 64 | 8 | `base_checkpoint_lsn` | Snapshot checkpoint base LSN. |
| 72 | 8 | `required_wal_start_lsn` | First WAL LSN required for recovery. Must be greater than or equal to `base_checkpoint_lsn`. |
| 80 | 8 | `format_fingerprint_offset` | Must be `320` in v0. |
| 88 | 4 | `format_fingerprint_count` | Must be nonzero and bounded by `DATABASE_MANIFEST_MAX_FINGERPRINTS`. |
| 92 | 4 | `format_fingerprint_entry_len` | Must equal `16`. |
| 96 | 8 | `segment_index_root_offset` | Must follow the fingerprint table exactly. |
| 104 | 4 | `segment_index_root_count` | Must be nonzero and bounded by `DATABASE_MANIFEST_MAX_SEGMENT_INDEX_ROOTS`. |
| 108 | 4 | `segment_index_root_entry_len` | Must equal `96`. |
| 112 | 8 | `extension_offset` | Must follow the segment index root table exactly. |
| 120 | 8 | `extension_len` | May be zero. Must be bounded. |
| 128 | 8 | `signature_offset` | Must follow extension bytes exactly. |
| 136 | 4 | `signature_len` | May be zero only for pre-persistence fixtures or explicit unsigned development policy. |
| 140 | 2 | `signature_algorithm_tag` | `0` when no signature is present; otherwise a known algorithm tag. |
| 142 | 2 | `reserved_142` | Must be zero. |
| 144 | 32 | `previous_manifest_hash` | SHA-256 hash of the previous accepted manifest file, or all zero for genesis. |
| 176 | 32 | `snapshot_descriptor_hash` | Nonzero SHA-256 hash binding the published snapshot descriptor. |
| 208 | 32 | `format_fingerprint_hash` | SHA-256 hash over the encoded fingerprint table. |
| 240 | 32 | `segment_index_roots_hash` | SHA-256 hash over the encoded segment index root table. |
| 272 | 32 | `catalog_root_hash` | SHA-256 hash of the catalog root descriptor used by this snapshot; all zero only before durable catalog storage is implemented. |
| 304 | 8 | `payload_crc64` | CRC-64 over bytes from offset 320 through the byte before the trailer. |
| 312 | 8 | `reserved_312` | Must be zero. |

The header CRC covers only the fixed header. The payload CRC covers the
fingerprint table, segment index root table, extension bytes, and signature
bytes. The trailer carries whole-file hash evidence.

### Manifest flag bits

| Bit | Name | Rule |
| ---: | --- | --- |
| `0x0000_0001` | `HAS_SIGNATURE` | Must be set when `signature_len > 0`. Must be clear when `signature_len == 0`. |
| `0x0000_0002` | `HAS_CATALOG_ROOT` | Must be set when `catalog_root_hash` is nonzero. |
| `0x0000_0004` | `FORENSIC_HOLD` | Marks a manifest retained for forensic policy. It does not authorize mutation. |
| `0x0000_0008` | `GENESIS_MANIFEST` | Allows all-zero `previous_manifest_hash`; must be clear after the first manifest in a chain. |

Unknown flag bits must be rejected by normal startup, backup, restore, and
publication readers. `ForensicStart` may report unknown bits read-only with
replay disabled.

### Signature algorithm tags

| Tag | Meaning | Rule |
| ---: | --- | --- |
| `0` | None | Allowed only when `signature_len == 0`. |
| `1` | Ed25519 over canonical manifest bytes | Preferred production tag. |
| `2` | ECDSA P-256 SHA-256 over canonical manifest bytes | Optional enterprise tag. |

Canonical signed bytes are the entire manifest file with these fields zeroed:

- `header_crc32`;
- `payload_crc64`;
- trailer `manifest_file_sha256`;
- trailer `trailer_crc32`;
- `SignatureBytes`.

Production policy should require a nonzero signature before accepting a new
manifest root. Development fixtures may use unsigned manifests only when the
startup or test policy explicitly says unsigned manifests are allowed.

### Storage format fingerprint entries

Each `FormatFingerprintEntryV0` is fixed at 16 bytes.

| Offset in entry | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 2 | `format_kind_tag` | Must be a known storage format kind. |
| 2 | 2 | `flags` | Must be zero in v0. |
| 4 | 4 | `format_major` | Must be nonzero. |
| 8 | 4 | `format_minor` | May be zero. |
| 12 | 4 | `entry_crc32` | CRC-32 over this 16-byte entry with bytes 12 through 15 zeroed. |

Known v0 format kind tags are:

| Tag | Format kind | Required before durable publication |
| ---: | --- | --- |
| 1 | `Page` | Yes. |
| 2 | `HeapPage` | Yes for heap-backed tables. |
| 3 | `BTreeKey` | Yes when any B-Tree index is present. |
| 4 | `BTreeNode` | Yes when any B-Tree index is present. |
| 5 | `WalRecord` | Yes for recovery compatibility evidence. |
| 6 | `WalPayload` | Yes for replay compatibility evidence. |
| 7 | `Manifest` | Yes. |
| 8 | `Segment` | Yes when cold segments are referenced. |
| 9 | `Checkpoint` | Yes when checkpoint metadata participates in the recovery floor. |
| 10 | `SegmentIndex` | Yes when `segment_index_root_count > 0`. |

Entries must be sorted by `format_kind_tag` and must not contain duplicates.
Unknown required format kinds fail closed before WAL redo. A reader may ignore
known optional kinds only when the extension policy for that kind states that
the artifact is not required for recovery.

### Segment index root entries

Each `SegmentIndexRootEntryV0` is fixed at 96 bytes. It binds the manifest to
one immutable segment index artifact. Most v0 manifests should use exactly one
root entry. Multiple entries are reserved for future sharding by object or
range and must still be validated as a complete, non-overlapping set.

| Offset in entry | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `segment_index_id` | Must be nonzero and unique in the table. |
| 8 | 8 | `snapshot_id` | Must equal the manifest header `snapshot_id`. |
| 16 | 8 | `first_segment_id` | Must be nonzero. |
| 24 | 8 | `segment_count` | Must be nonzero. |
| 32 | 8 | `segment_index_byte_len` | Must equal the referenced `SegmentIndex v0` file length. |
| 40 | 32 | `segment_index_sha256` | SHA-256 hash of the referenced `SegmentIndex v0` file. |
| 72 | 8 | `first_page_id` | Must be nonzero. |
| 80 | 8 | `last_page_id` | Must be greater than or equal to `first_page_id`. |
| 88 | 4 | `entry_crc32` | CRC-32 over this 96-byte entry with bytes 88 through 91 zeroed. |
| 92 | 4 | `flags` | Must be zero in v0. |

The segment index root table must be sorted by `segment_index_id`. If multiple
roots exist, their page ranges must not overlap unless a future extension marks
the entries as disjoint object-space shards and supplies a validated shard
identity.

### Extension bytes

`extension_len` may be zero. When nonzero, extension bytes must use a
type-length-value encoding:

```text
ExtensionRecord =
    kind_tag: u16 little-endian
    flags: u16 little-endian
    length: u32 little-endian
    payload: u8[length]
```

Unknown extension records with the required flag set must be rejected. Unknown
extension records without the required flag may be skipped only after their
length is checked against `DATABASE_MANIFEST_MAX_EXTENSION_BYTES`.

Extensions are covered by `payload_crc64`, trailer hash, and signature bytes.
No extension may override fixed header fields.

### Manifest trailer layout

The encoded `ManifestTrailerV0` is fixed at 96 bytes and is placed at
`total_len - 96`.

| Offset in trailer | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `payload_crc64_mirror` | Must equal header `payload_crc64`. |
| 8 | 32 | `manifest_file_sha256` | SHA-256 over the full file with this field zeroed. |
| 40 | 32 | `manifest_chain_hash` | SHA-256 over previous manifest hash, database id, manifest version, snapshot id, recovery floor, and `manifest_file_sha256`. |
| 72 | 4 | `trailer_crc32` | CRC-32 over this 96-byte trailer with bytes 72 through 75 zeroed. |
| 76 | 4 | `trailer_flags` | Must be zero in v0. |
| 80 | 16 | `reserved_80` | Must be zero. |

Readers must reject a zero `payload_crc64_mirror`, zero `manifest_file_sha256`,
zero `manifest_chain_hash`, or zero `trailer_crc32`.

### Decode-before-allocate

Manifest decoders must avoid unbounded allocation from untrusted length fields.

Required order:

1. Verify that at least `DATABASE_MANIFEST_HEADER_LEN +
   DATABASE_MANIFEST_TRAILER_LEN` bytes are available.
2. Decode the fixed 320-byte header.
3. Validate magic, version, byte-order marker, fixed header length, flags,
   identity fields, LSN relationship, table offsets, table lengths, signature
   bounds, extension bounds, and header CRC.
4. Use checked arithmetic to compute the fingerprint table, segment index root
   table, extension, signature, and trailer ranges.
5. Require `total_len` to equal the input length when decoding a standalone
   manifest file.
6. Decode fingerprint entries without allocating more than
   `DATABASE_MANIFEST_MAX_FINGERPRINTS`.
7. Decode segment index root entries without allocating more than
   `DATABASE_MANIFEST_MAX_SEGMENT_INDEX_ROOTS`.
8. Validate per-entry CRCs, ordering, uniqueness, nonzero required fields, and
   cross-field relationships.
9. Recompute `format_fingerprint_hash`, `segment_index_roots_hash`,
   `payload_crc64`, trailer CRC, and `manifest_file_sha256`.
10. Verify signature policy before allowing the manifest to become the active
    root.

No caller may expose a decoded manifest as accepted recovery state before all
required checks pass.

### Publication and manifest switch

Manifest publication is a replace-by-publish operation. It must never update an
active cold snapshot in place.

The required order is:

1. Build the new cold snapshot and segment indexes under staging paths.
2. Validate page, segment, and `SegmentIndex v0` hashes and checksums.
3. Write and fsync every referenced segment index artifact.
4. Write and fsync the new manifest artifact.
5. Append and durably flush the WAL `ManifestSwitch` record, or an equivalent
   durable manifest switch record accepted by the WAL specification.
6. Atomically switch the active manifest root pointer.
7. Fsync the parent directory or root-pointer container.
8. Defer cleanup of old manifests and snapshots until recovery, backup,
   replica, and forensic retention policies allow it.

A crash must leave either the old manifest or the new manifest valid. It must
not leave a half-manifest or half-segment-index as the active root.

### Recovery behavior

Normal startup and `SafeStart` must:

- read the active manifest root pointer;
- decode and validate `DatabaseManifest v0`;
- validate storage format fingerprints before WAL redo;
- validate every referenced `SegmentIndex v0` artifact by length and SHA-256;
- validate snapshot availability before choosing a recovery base;
- require `required_wal_start_lsn >= base_checkpoint_lsn`;
- require durable WAL coverage from `required_wal_start_lsn` through the
  needed recovery target before making recovered state visible;
- reject unknown manifest, page, heap, B-Tree, WAL payload, segment, or segment
  index formats before mutation replay.

If the active manifest is corrupt, startup may try a previous manifest only
when the root-pointer policy, chain hash, retention policy, and operator mode
allow it. If no compatible manifest and snapshot pair is available, normal
startup must refuse or enter read-only `ForensicStart` according to policy.

`ForensicStart` may inspect unknown or corrupt manifests read-only. It must not
append WAL, publish a manifest, repair segment indexes in place, perform redo,
perform undo, rebuild indexes as accepted state, or allow Application-surface
traffic.

### Compatibility rules

The v0 reader accepts only:

- `format_major == 1`;
- `format_minor == 0`;
- `byte_order == 0x0102`;
- fixed header, entry, and trailer lengths from this spec;
- known required flag bits;
- known required storage format kinds;
- known signature algorithm tags allowed by policy.

A future minor version may be backward-compatible only if:

- `format_major` remains `1`;
- every new fixed field is appended through extension records or reserved
  fields that v0 readers already require to be zero;
- v0 readers can skip non-required extension records after validating length;
- hashes, CRCs, signatures, root-pointer behavior, and recovery floors remain
  canonical.

Any major version change, fixed-offset reinterpretation, endian change, table
entry length change, checksum algorithm change, signature input change, or
recovery-floor semantic change is incompatible and requires a new manifest
format spec plus migration, dual-read, or reject policy.

Pre-persistence fixtures without this format must be rejected by release
readers unless an explicit migration reader is implemented and crash-tested.

## Validation

Documentation acceptance checks:

- The spec defines explicit little-endian fields and fixed offsets.
- The spec states that Rust native struct serialization is forbidden.
- The spec preserves WAL-before-visible-commit and WAL-before-manifest-switch
  publication.
- The spec requires storage format validation before WAL redo.
- The spec binds manifests to `SegmentIndex v0` by byte length and SHA-256.
- The spec defines checksum, hash, signature, compatibility, and recovery
  behavior.
- The spec does not introduce SQL, gRPC, runtime JSON defaults, GPU critical
  path behavior, or Application-surface administration.

Future implementation work should add or keep owner tests for:

```powershell
cargo test -p andromeda-storage manifest
cargo test -p andromeda-storage --test layout_publication_contract
cargo test -p andromeda-storage --test wal_scan_recovery_contract
cargo test -p andromeda-storage --test file_wal_recovery_contract
cargo test -p andromeda-storage --test recovery_completeness_contract
```

Future byte-codec work must add:

```powershell
cargo test -p andromeda-storage --test database_manifest_golden_vectors
cargo test -p andromeda-storage --test database_manifest_corruption_contract
cargo check --manifest-path fuzz/Cargo.toml --bin manifest_decode --locked
```

Sustained fuzz evidence for `manifest_decode` must include valid current
manifest bytes, empty input, truncated headers, bad versions, unsupported
flags, length overflow, duplicate fingerprints, fingerprint hash mismatch,
segment index hash mismatch, bad signature length, trailer truncation, and
checksum mismatch.

### Validation matrix

| Area | Positive case | Negative case | Required outcome |
| --- | --- | --- | --- |
| Magic | `ANDMAN0\0` | Any other bytes | Reject before table decode. |
| Version | Major 1, minor 0 | Unknown major or unsupported minor | Reject normal startup. |
| Byte order | `0x0102` | Any other marker | Reject. |
| Header length | 320 | Any other value | Reject. |
| Trailer length | 96 by `total_len` placement | Truncated or extra bytes | Reject. |
| Identity | Nonzero database, manifest, epoch, snapshot | Any required zero identity | Reject. |
| LSN relationship | `required_wal_start_lsn >= base_checkpoint_lsn` | WAL start before checkpoint | Reject. |
| Flags | Known bits and consistent signature/catalog state | Unknown bit or inconsistent state | Reject. |
| Fingerprints | Sorted, unique, nonzero versions | Duplicate, unknown required kind, bad CRC | Reject before redo. |
| Segment roots | Sorted, unique, matching snapshot | Duplicate, overlap, missing hash, bad CRC | Reject. |
| Hashes | Recomputed table and file hashes match | Hash mismatch | Reject. |
| Signature | Valid per policy | Missing when required or invalid | Reject publication and normal startup. |
| Manifest switch | New manifest and WAL switch durable before root change | Crash before durable switch | Reopen old manifest or refuse; never accept partial state. |
| Recovery | Manifest plus segment index plus WAL coverage valid | Unknown format or WAL gap | Reject normal replay; forensic read-only only. |

### Crash validation gates

Manifest publication is not release-ready until deterministic crash tests cover:

| Crash point | Required result |
| --- | --- |
| Before segment index fsync | Old manifest remains active; staged bytes ignored. |
| After segment index fsync, before manifest fsync | Old manifest remains active; no half-manifest accepted. |
| After manifest fsync, before durable `ManifestSwitch` WAL | Old manifest remains active unless explicit recovery policy proves otherwise. |
| After durable `ManifestSwitch` WAL, before root pointer switch | Recovery may complete the switch only if the new manifest and segment index validate. |
| During root pointer switch | Old or new manifest is selected; never a torn pointer or half-manifest. |
| After root pointer switch, before cleanup | New manifest validates; old manifest remains available until retention policy allows cleanup. |
| Manifest corrupted after switch | Previous valid manifest may be used only with chain, retention, and policy evidence. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Decoder accepts a manifest by transmuting a Rust struct. | Native layout was treated as durable layout. | Replace with explicit offset-based little-endian codec. |
| Startup replays WAL before checking fingerprints. | Recovery format gate was bypassed. | Validate manifest, format fingerprints, and segment indexes before redo. |
| `required_wal_start_lsn` is lower than `base_checkpoint_lsn`. | Manifest recovery floor is inconsistent. | Reject the manifest and use a previous valid manifest or forensic mode. |
| Segment index SHA-256 mismatch. | The referenced index file changed or the manifest points to the wrong artifact. | Reject startup or restore from a valid manifest and snapshot pair. |
| Duplicate fingerprint kind appears. | Storage format table drifted. | Reject the manifest and regenerate from a canonical sorted fingerprint set. |
| Signature bytes are present but `HAS_SIGNATURE` is clear. | Header flags and signature section disagree. | Reject and fix the manifest writer. |
| Manifest cleanup removes the previous valid snapshot too early. | Retention policy ignored recovery, backup, replica, or forensic pins. | Restore retention checks before cleanup. |
| `ForensicStart` publishes a repaired manifest. | Forensic mode mutated inspected truth. | Reject behavior; forensic inspection must be read-only. |

## References

- `AGENTS.md`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/governance/decisions/DEC-032-storage-format-gate.md`
- `documentations/specs/PageHeader_PageTrailer_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `documentations/specs/SegmentIndex_v0.md`
- `fuzz/ROADMAP_FUZZ_GAPS_2026.md`
- `fuzz/VALIDATION_MATRIX.md`
- `crates/andromeda-storage/src/manifest.rs`
- `crates/andromeda-storage/src/manifest/format.rs`
- `crates/andromeda-storage/src/manifest/hash.rs`
- `crates/andromeda-storage/src/manifest/snapshot.rs`
- `crates/andromeda-storage/src/segment.rs`
- `crates/andromeda-storage/src/cold_store.rs`
- `crates/andromeda-storage/tests/layout_publication_contract.rs`
- `crates/andromeda-storage/tests/core_io_gates.rs`
