# andromeda-wal

## Purpose

`andromeda-wal` owns Andromeda write-ahead log primitives, log sequence numbers, WAL record bounds, typed frame codec wrappers, durable-prefix scanning, transaction classification, durability fence helpers, commit-log durability evidence, in-memory WAL summaries, and the physical `FileWal` byte contract.

This crate protects the durable ordering rule that a transaction commit cannot become visible until its commit record is durably flushed to WAL. It also protects the byte-level evidence that storage and recovery use to decide what can be replayed after a crash.

## Scope

This crate owns:

- `Lsn`, `WalRecord`, `WalRecordHeader`, `WalRecordKind`, and WAL segment descriptors.
- Typed WAL frame wrappers over the raw `andromeda-wal-codec` byte contract, plus file-header codecs with little-endian canonical serialization.
- Record size, batch size, segment boundary, LSN ordering, and previous-LSN chain validation.
- Durable-prefix scan behavior, recoverable-tail detection, and non-recoverable chain-break rejection.
- `FileWal` open, append, flush-through, durable header, scan, and replay surfaces.
- Durability fence helpers for page flushes, manifest switches, and recovery floors.
- Commit log entries and the commit boundary that enforce durable WAL before visibility publication.

The crate uses typed errors and `AndromedaResult` failures to make short flushes, invalid bytes, broken chains, and unsafe durability states classifiable by callers.

## Non-goals

This crate does not own:

- Storage recovery reports, heap redo, catalog replay bridges, startup policy, page integration, or durable visibility publication.
- Transaction status tables, MVCC visibility, lock behavior, or final visibility publication.
- Procedure dispatch, SRPL execution, QUIC transport, or application-facing SQL.
- Benchmark, GPU, analytics, or learned-model outputs as WAL truth.

Downstream crates must not use `andromeda-wal` as a place for generic storage, transaction, or recovery policy. Those policies belong at their owning crate boundaries.

## Prerequisites

Before changing this crate, confirm that the change respects these requirements:

- WAL bytes are encoded and decoded through explicit codecs, not native Rust layout serialization.
- `flush_through` cannot report success unless the durable prefix covers the requested LSN.
- Durable scans expose only the verified prefix and classify recoverable tails without accepting broken chains.
- Record bounds keep WAL growth observable, bounded, versioned, and rejectable.
- Commit, rollback, and incomplete transaction classification uses durable records only.
- Any change to the byte contract includes golden, roundtrip, corruption-rejection, and property coverage.

## Procedure

1. Identify whether the change affects logical WAL records, binary frame format, `FileWal`, scan behavior, transaction classification, or durability fences.
2. Keep format changes explicit. Update constants, version checks, field offsets, checksums, and little-endian encode/decode logic together.
3. Preserve durable-prefix semantics. Unflushed records must not be visible to replay, recovery, commit publication, or storage flush decisions.
4. Return typed errors or `AndromedaResult` failures for invalid WAL structure, short durability, broken chains, and boundary violations.
5. Add or update tests that cover exact bytes, valid and invalid scan prefixes, truncated tails, chain breaks, and transaction terminal-state classification.
6. Coordinate storage-facing behavior with `andromeda-storage`; storage remains responsible for recovery reports, manifest integration, page integration, and final visibility decisions.

## Validation

Recommended WAL owner gates:

```powershell
cargo test -p andromeda-wal --test wal_codec_contract -- --nocapture
cargo test -p andromeda-wal --test property_wal_roundtrip -- --nocapture
cargo test -p andromeda-wal --test wal_record_bounds_contract -- --nocapture
cargo test -p andromeda-wal --test wal_durability_fence_contract -- --nocapture
cargo test -p andromeda-wal --test wal_compaction_contract -- --nocapture
cargo test -p andromeda-wal --test wal_gc_four_boundaries_integration -- --nocapture
cargo test -p andromeda-wal --test wal_gc_snapshot_protection -- --nocapture
cargo test -p andromeda-wal --test file_wal_contract -- --nocapture
cargo test -p andromeda-wal --test api_compat -- --nocapture
```

For cross-crate WAL integration, add storage and topology evidence:

```powershell
cargo test -p andromeda-storage --test wal_ownership_invariants -- --nocapture
cargo test -p andromeda-recovery --test file_wal_recovery_contract -- --nocapture
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

For durable byte changes, add fuzz coverage for frame decoding and scan-prefix rejection before accepting the change.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `FileWal::open` rejects a file | Inspect header magic, format version, durable byte count, durable record count, and scan stop reason. |
| A flush returns an older LSN than requested | Treat it as a failed durability boundary; callers must not publish commit visibility. |
| Recovery sees an incomplete transaction | Check whether durable records contain begin without a single terminal commit or rollback record. |
| A truncated tail appears in scan output | Confirm the valid prefix remains replayable and the tail is classified as recoverable only when chain integrity holds. |
| A codec test fails after adding fields | Update versioning, field offsets, checksum coverage, golden vectors, and compatibility expectations together. |

## References

- `src/lib.rs`
- `src/lsn.rs`
- `src/wal_codec.rs`
- `src/file_wal.rs`
- `src/file_wal/`
- `src/write_ahead_log/commit_log_entry.rs`
- `src/write_ahead_log/commit_log_facade.rs`
- `src/write_ahead_log/durability_fence.rs`
- `src/write_ahead_log/record_bounds.rs`
- `src/write_ahead_log/gc.rs`
- `src/write_ahead_log/gc_eligibility.rs`
- `src/write_ahead_log/compaction.rs`
- `src/write_ahead_log/transaction.rs`
- `tests/wal_codec_contract.rs`
- `tests/property_wal_roundtrip.rs`
- `tests/wal_record_bounds_contract.rs`
- `tests/wal_durability_fence_contract.rs`
- `tests/wal_compaction_contract.rs`
- `tests/wal_gc_four_boundaries_integration.rs`
- `tests/wal_gc_snapshot_protection.rs`
- `tests/file_wal_contract.rs`
- `tests/api_compat.rs`
