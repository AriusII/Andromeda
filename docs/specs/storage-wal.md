# Storage And WAL

## Purpose

This spec defines durable WAL, page, segment index, database manifest, and
buffer pool contracts. It preserves the rule that durable WAL precedes visible
commit and dirty page flush.

## Global storage rules

- Persistent bytes use explicit offset-based codecs and little-endian fields
  unless a specific format says otherwise.
- Rust struct memory layout must never be used as persistent format.
- Decoders validate fixed headers, versions, byte order, lengths, flags,
  checksums, digests, and bounds before allocating from untrusted lengths.
- Recovery must validate manifest, format fingerprints, segment indexes, and
  WAL boundaries before normal redo.
- Forensic inspection is read-only and must not repair or publish inspected
  artifacts in place.

## WAL record format

`andromeda-wal` owns the canonical WAL frame codec, file WAL header codec, WAL
scan engine, and public WAL byte-format constants.

| Constant | Value | Rule |
| --- | ---: | --- |
| `WAL_FORMAT_VERSION` | `1` | Incompatible frame changes require a version bump. |
| `WAL_BYTE_ORDER_LITTLE_ENDIAN` | `0x0102` | File WAL header must carry this marker. |
| `WAL_RECORD_HEADER_LEN` | `72` | Fixed frame header length. |
| `WAL_RECORD_SIZE_LIMIT` | `1 MiB` | Maximum payload size before append. |
| `WAL_SEGMENT_BOUNDARY` | `4 MiB` | Current cumulative batch boundary for segment accounting. |
| `WAL_BATCH_ROW_LIMIT` | `256` | Row-operation count limit per transaction batch. |
| `FILE_WAL_HEADER_LEN` | `80` | Fixed current file WAL header length. |
| `FILE_WAL_MONO_SEGMENT_ID` | `1` | Current file WAL is explicitly mono-segment. |

The frame header carries magic, format version, header length, total length,
record kind, flags, LSN, optional previous LSN, optional transaction id,
payload length, record checksum, and header checksum. Unknown flags, unknown
record kinds, zero required LSNs, payload overflows, and checksum failures are
rejected before replay.

Accepted record kind tags include transaction begin, commit, rollback; page,
row, index, MVCC, map, checkpoint, snapshot, manifest switch, catalog change,
security audit, and B-Tree mutation records. Adding a kind is a compatibility
event and must update the WAL spec version or a decision record.

WAL scans stop at the first invalid frame or chain break. Tail truncation may
preserve the prior valid prefix. Middle corruption, LSN gaps, duplicate or
reordered LSNs, and previous-LSN mismatches reject normal open or replay.

## Page format

The current page domain model and codec intentionally distinguish the 96-byte
minimum domain header from the current 112-byte encoded header image.

| Constant | Value | Rule |
| --- | ---: | --- |
| `PageHeader::MAGIC` | `0x414e4452` | Required little-endian page magic. |
| `PageHeader::FORMAT_VERSION_V0` | `1` | Current page header version. |
| `PageHeader::MIN_HEADER_LEN_V0` | `96` | Minimum conceptual domain header length. |
| `PAGE_CODEC_V1_HEADER_LEN` | `112` | Fixed encoded header length. |
| `PAGE_CODEC_V1_TRAILER_LEN` | `48` | Fixed encoded trailer length. |
| Heap payload offset | `112` | First byte heap tuple payload may occupy in the current heap page candidate. |

Accepted page size tags are `1` for 16 KiB and `2` for 32 KiB. Accepted page
types are `FixedRow`, `HybridRow`, `Manifest`, and `Free`. Unknown page size,
type, and flag bits are rejected.

The encoded header includes page id, object id, allocation id, page LSN, page
epoch, optional previous and next page ids, payload bounds, free-space bounds,
slot count, row count, header CRC, and header integrity CRC. The encoded
trailer includes payload CRC64, payload SHA-256, and one torn-write guard.

Page validation rejects unknown formats before redo, corrupt payloads, torn or
mixed page images, ambiguous slot counts, and page flushes beyond durable WAL
coverage.

## Page lifecycle and buffer pool

| Area | Rule |
| --- | --- |
| Allocation | Page identity and layout require WAL evidence before visible database state. |
| Residency | RAM copies are cache state only; they are not durable truth. |
| Dirty tracking | Dirty mutation requires a live pin and a valid nonzero LSN. |
| Flush | Dirty page flush requires durable WAL coverage for the page's latest dirty LSN. |
| Eviction | Pinned frames and dirty frames without successful flush are not reusable. |
| Checkpoint | Checkpoint begin WAL is durable before checkpoint flush; checkpoint end WAL and metadata publish only after required flush evidence. |
| Read-ahead | Advisory clean prefetch only; it must not affect visibility or commit, rollback, WAL, recovery, catalog, or security behavior. |

The buffer pool must report blocked dirty pages when WAL is behind and must
keep them dirty. `flush_all_dirty` must not silently accept dirty pages when no
WAL durability observer is available.

## Segment index

`SegmentIndex v0` binds published cold segment artifacts to a parent database
manifest. It is not a mutable segment catalog.

| Constant | Value | Rule |
| --- | ---: | --- |
| Magic | `ANDSGIX0` | Reject any other bytes before entry decode. |
| Format | Major `1`, minor `0` | Unknown major or unsupported minor rejects normal startup. |
| Header length | `256` | Fixed. |
| Entry length | `160` | Fixed. |
| Trailer length | `96` | Fixed by `total_len` placement. |
| Max entries | `16,777,216` | Decode and validation bound. |
| Published state | `PublishedCold` tag `3` | Required for every manifest-referenced entry. |

Segment index entries must be sorted, non-overlapping, nonzero, and bound to
the parent manifest by snapshot id, manifest version, byte length, SHA-256, and
root evidence. Segment artifact CRC and SHA-256 mismatches reject normal
startup.

## Database manifest

`DatabaseManifest v0` is the durable recovery root for a database snapshot and
its required WAL floor.

| Constant | Value | Rule |
| --- | ---: | --- |
| Magic | `ANDMAN0\0` | Reject any other bytes before table decode. |
| Format | Major `1`, minor `0` | Unknown major or unsupported minor rejects normal startup. |
| Header length | `320` | Fixed. |
| Trailer length | `96` | Fixed by `total_len` placement. |
| Fingerprint entry length | `16` | Fixed. |
| Segment index root entry length | `96` | Fixed. |
| Max fingerprints | `256` | Decode and validation bound. |
| Max segment index roots | `4096` | Decode and validation bound. |
| Max extension bytes | `1 MiB` | Decode bound. |
| Max signature bytes | `4096` | Decode bound. |

Manifest identity fields must be nonzero. `required_wal_start_lsn` must be
greater than or equal to `base_checkpoint_lsn`. Format fingerprints are sorted,
unique, and required for all storage subformats needed by recovery. Required
hashes are SHA-256 over canonical byte ranges and must not be all zero.

Manifest publication requires new segment indexes and the manifest to be
durable before a root pointer switch. The `ManifestSwitch` WAL evidence must be
durable before recovery completes a switch that was interrupted. Startup must
select the old or new manifest; it must never accept a torn pointer or
half-manifest.

## Crash and recovery gates

- Before segment index fsync, the old manifest remains active.
- After segment index fsync but before manifest fsync, the old manifest remains
  active and the new index is unreachable.
- After manifest fsync but before durable `ManifestSwitch` WAL, the old
  manifest remains active unless explicit recovery policy proves otherwise.
- After durable `ManifestSwitch` WAL but before root pointer switch, recovery
  may complete the switch only if the new manifest and segment index validate.
- Published segment corruption rejects normal startup; recovery uses a valid
  previous manifest, restore, or read-only forensic inspection.

## Validation gates

- WAL golden vectors must cover fixed header fields, flags, checksums,
  LSN continuity, durable prefix handling, and malformed input rejection.
- Page tests must distinguish domain header length from encoded header length.
- Storage tests must prove WAL-before-page-flush.
- Manifest and segment index tests must reject unknown formats before redo.
- Crash tests must prove no partial manifest or segment index is accepted.
