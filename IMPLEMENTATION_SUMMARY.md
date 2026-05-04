# Storage Crates Refactoring — Archived Implementation Summary

> **Status:** Archived. The phased narrative previously kept here is stale and
> contradicts the current source tree (it still flags `wal.rs` as "broken",
> references the never-shipped `mvcc_refactored.rs` and `wal_clean.rs`, and
> lists `placement.rs`, `recovery.rs`, `file_wal.rs`, `wal_codec.rs`, and
> `operational_profile.rs` as TODO although each is now split into a
> sub-directory and re-exported from the root module).
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
>
> No live work item depends on the prior content of this file.

<!-- Historical narrative removed; see notice above. -->


<!-- end of archived notice -->

