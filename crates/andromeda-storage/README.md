# andromeda-storage

## Purpose

`andromeda-storage` owns Andromeda storage integration for pages, heap storage, B+Tree access paths, buffer-pool durability fences, manifests, backup and restore planning, HA/DR storage evidence, and crash recovery.

The crate is part of the C5 durable kernel. A storage change must treat durable bytes, WAL coverage, recovery reports, and publication evidence as the source of truth. RAM state, temp files, benchmark output, and GPU or analytics output are advisory only and must not decide commit visibility or recovery outcomes.

## Scope

This crate owns:

- Page, heap, B+Tree, extent, segment, manifest, placement, and buffer-pool integration.
- WAL-before-page-flush checks and durable visibility integration with `andromeda-wal` and `andromeda-tx`.
- Recovery planning, replay selection, startup mode evidence, catalog WAL replay integration, backup artifacts, restore orchestration, and PITR-oriented evidence.
- Storage-facing typed errors such as `DiskManagerError`, `BufferPoolError`, and storage `AndromedaResult` failures that preserve error kind and recovery context.
- Compatibility reexports that let older callers migrate toward the dedicated WAL owner crate without breaking API tests.

Every durable format in this crate must use explicit byte codecs. Do not serialize Rust native structs directly to disk or network.

## Non-goals

This crate does not own:

- Procedure contracts, SRPL parsing, or application-facing execution.
- Application-facing ad hoc SQL or dynamic table-name dispatch.
- WAL byte-contract ownership that now belongs in `andromeda-wal`, except for storage integration and compatibility reexports.
- Transaction status authority that belongs in `andromeda-tx`, except where storage validates durable replay and visibility evidence.
- GPU, SIMD, benchmark, or learned-model output as commit, rollback, recovery, MVCC visibility, catalog publication, or security truth.

## Prerequisites

Before changing this crate, confirm that the change respects these requirements:

- The commit record is durably flushed to WAL before any storage state becomes visible.
- Page flushes prove WAL durability through the page LSN.
- Manifest switches prove checkpoint and recovery-floor coverage against the durable WAL prefix.
- Recovery code consumes durable WAL records and durable artifacts, not in-memory summaries.
- Error paths return typed errors or `AndromedaResult` values with stable error kinds and actionable messages.
- Persistent page, heap, B+Tree, manifest, backup, restore, and recovery formats have roundtrip, golden, corruption-rejection, and crash/recovery coverage.

## Procedure

1. Identify the storage boundary being changed: page, heap, B+Tree, buffer pool, manifest, backup, restore, HA/DR, or recovery.
2. Find the durable byte contract and the recovery evidence affected by the change.
3. Update the smallest module that owns that responsibility. Keep `src/lib.rs` limited to module declarations and intentional reexports.
4. Preserve WAL-before-visibility ordering. A visible commit, manifest publication, page flush, backup checkpoint, or replay decision must be backed by durable WAL evidence.
5. Use explicit codecs for persistent bytes. Add version fields, little-endian encoding, checksums or hashes, and compatibility gates where the format is durable.
6. Return typed errors for invalid durability, format, replay, or recovery states. Do not hide failures behind strings that callers cannot classify.
7. Add tests near the affected behavior. Use property tests for parser/codec invariants, fuzz targets for byte decoders, and deterministic crash/recovery scenarios for startup and replay behavior.

## Validation

Use the narrowest command that proves the changed boundary, then run broader workspace gates before accepting a source change.

Recommended storage gates:

```powershell
cargo test -p andromeda-storage --test wal_durability_fence_contract -- --nocapture
cargo test -p andromeda-storage --test file_wal_recovery_contract -- --nocapture
cargo test -p andromeda-storage --test recovery_contract -- --nocapture
cargo test -p andromeda-storage --test recovery_completeness_contract -- --nocapture
cargo test -p andromeda-storage --test property_page_codec_v1 -- --nocapture
cargo test -p andromeda-storage --test property_page_parsing -- --nocapture
cargo test -p andromeda-storage --test property_recovery_replay -- --nocapture
cargo test -p andromeda-storage --test wal_ownership_invariants -- --nocapture
```

For durable format or recovery changes, add the relevant crash/recovery, fuzz, Miri, or property validation before merging. For cross-crate ownership changes, also run:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-storage --test api_compat_reexports -- --nocapture
```

## Troubleshooting

| Symptom | Check |
| --- | --- |
| A page flush fails with a WAL durability error | Verify the durable WAL LSN is greater than or equal to the page LSN before flush. |
| Recovery replays fewer records than expected | Check scan boundaries, previous-LSN continuity, recovery floor, and truncated-tail classification. |
| A manifest switch is rejected | Confirm manifest checkpoint evidence is covered by the durable WAL prefix. |
| A catalog or storage publication appears after restart but not before restart | Compare visible publication evidence with durable WAL replay and audit records. |
| A decoder accepts corrupt durable bytes | Add corruption-rejection tests and keep the codec explicit rather than struct-derived. |

## References

- `src/lib.rs`
- `src/page/store.rs`
- `src/buffer_pool/wal_durability.rs`
- `src/manifest.rs`
- `src/recovery.rs`
- `src/recovery/`
- `src/backup/`
- `src/restore_orchestration/`
- `tests/wal_durability_fence_contract.rs`
- `tests/file_wal_recovery_contract.rs`
- `tests/recovery_contract.rs`
- `tests/recovery_completeness_contract.rs`
- `tests/property_page_codec_v1.rs`
- `tests/property_recovery_replay.rs`
