# Storage Crates Refactoring — Archived Report

> **Status:** Archived. The wave-era report previously kept here is superseded
> by the current source tree and by `crates/andromeda-storage/REFACTORING_PROGRESS.md`.
>
> Do not consult the historical narrative for current module ownership.
> Authoritative references are:
>
> * `crates/andromeda-storage/src/lib.rs` — module roots and re-export surface.
> * `crates/andromeda-storage/src/wal.rs` and
    > `crates/andromeda-storage/src/write_ahead_log/mod.rs` — write-ahead log
    > ownership table.
> * `crates/andromeda-storage/src/file_wal.rs`,
    > `recovery.rs`, `wal_codec.rs`, `placement.rs`, and `operational_profile.rs`
    > — each re-exports its split submodules.
> * `crates/andromeda-tx/src/mvcc.rs` — MVCC compatibility facade over
    > `mvcc_snapshot`, `mvcc_status`, and `mvcc_version`.
>
> No live work item depends on the prior content of this file.

<!-- Historical narrative removed; see notice above. -->


<!-- end of archived notice -->

