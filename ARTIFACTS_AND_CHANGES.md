# Refactoring Artifacts & Change Log — Archived

> **Status:** Archived. The wave-era change log previously kept here is stale
> and contradicts the current source tree. It still lists `wal.rs` as
> "DAMAGED", references files that were never committed
> (`mvcc_refactored.rs`, `wal_replacement.rs`, `wal_clean.rs`), and quotes
> "65–70% complete" status against work that is already shipped.
>
> Authoritative references for current module layout:
>
> * `crates/andromeda-storage/src/lib.rs`
> * `crates/andromeda-storage/src/wal.rs` and
    > `crates/andromeda-storage/src/write_ahead_log/mod.rs`
> * `crates/andromeda-storage/src/file_wal.rs`, `recovery.rs`, `wal_codec.rs`,
    > `placement.rs`, `operational_profile.rs`
> * `crates/andromeda-tx/src/mvcc.rs` (facade over `mvcc_snapshot`,
    > `mvcc_status`, `mvcc_version`)
> * `crates/andromeda-storage/REFACTORING_PROGRESS.md` for the current
    > storage-crate progress note.
>
> No live work item depends on the prior content of this file.

<!-- Historical narrative removed; see notice above. -->

<!-- end of archived notice -->

<!-- archived stale change-log content removed -->

