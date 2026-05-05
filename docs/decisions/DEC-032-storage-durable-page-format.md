# DEC-032: Storage Durable Page Format Baseline

**Status:** ACCEPTED  
**Date:** Q1 2026  
**Task:** STOR-FND-001 — Create storage format decision record; STOR-FND-002 — Define storage module ownership policy  
**Owner:** Storage Engine Architect  
**Scope:** HotStore/ColdStore page images, heap pages, future B+Tree pages, and WAL redo payload ownership before durable format code lands.

---

## Context

Andromeda needs one durable storage-format baseline before C2/N1/N2 page, heap, and B+Tree implementation work begins. Existing Rust contracts already define `PageHeader`, `PageTrailer`, `PageSize`, `PageType`, `PageFlags`, `WalRecord`, `WalFrameHeader`, and WAL frame constants. This DEC freezes the policy those implementations must follow and records the explicit boundaries that remain deferred to binary-format and recovery-specific work.

### Facts

- `crates/andromeda-storage/src/page.rs` is the canonical page contract owner; `layout/page.rs` is only a facade re-export.
- `crates/andromeda-storage/src/lsn.rs` is the canonical `Lsn` owner; it is re-exported at the storage crate root.
- `crates/andromeda-storage/src/write_ahead_log/record.rs` is the canonical `WalRecord`, `WalRecordHeader`, and `WalRecordKind` owner; `write_ahead_log`, legacy `wal`, and crate-root exports are facades.
- `DEC-014` requires focused modules behind `src/lib.rs`, with intentional public surface exposed by crate-root re-exports rather than consumer coupling to internal module paths.
- Existing page sizes are `KiB16` and `KiB32`.
- Existing page format version is `PageHeader::FORMAT_VERSION_V0 == 1`, with `MIN_HEADER_LEN_V0 == 96`.
- Existing trailer length is `PageTrailer::V0_LEN == 48`.
- Existing page types are `FixedRow`, `HybridRow`, `Manifest`, and `Free`.
- Existing WAL frames are versioned independently through `WAL_FORMAT_VERSION == 1` and carry opaque `WalRecord.payload` bytes plus checksums.

### Assumptions for this baseline

- The final byte-for-byte page-header serialization, numeric page-type tags, and endian table remain a binary-format deliverable, but they must preserve the contracts below.
- Heap and B+Tree implementations may add code-level helpers, but must not introduce a second durable page-header/trailer definition.

---

## Decision

### 1. Storage module ownership and facade policy

Storage durable type ownership is single-source. Future `buffer_pool`, `heap`, and `btree`
work must be added behind focused implementation modules without creating second durable
definitions in facades or peer modules.

#### Public versus internal modules

- `page` remains an internal implementation module declared as `mod page;` from
  `crates/andromeda-storage/src/lib.rs`. Its intentional public contract is re-exported from
  the storage crate root and through the existing `layout::page` compatibility facade.
- `lsn` remains an internal implementation module declared as `mod lsn;`; `Lsn` is re-exported
  from the storage crate root.
- `write_ahead_log` remains the public WAL domain facade (`pub mod write_ahead_log;`) because
  WAL record construction, scanning, shipping, and compatibility imports intentionally need a
  named facade. Its durable record taxonomy is owned only by `write_ahead_log::record`.
- The legacy `wal` module remains an internal compatibility facade declared as `mod wal;` and
  re-exported from the crate root. It must not define WAL record types.
- Future `buffer_pool`, `heap`, and `btree` modules must be declared crate-private
  (`mod buffer_pool;`, `mod heap;`, `mod btree;`) unless a later accepted DEC explicitly makes
  one of them a public facade. Public APIs from those modules must be exposed by narrow
  crate-root re-exports, following `DEC-014`.
- Public facade modules may re-export canonical items only. They must not own durable structs,
  enums, numeric tags, serialization tables, or independent validation rules.

#### Canonical durable type ownership

| Durable type or contract | Canonical owner | Allowed public exposure | Non-owners that must reuse it |
| --- | --- | --- | --- |
| `PageId` | `crate::page` | crate root; `crate::layout::page` re-export | `buffer_pool`, `heap`, `btree`, WAL payload codecs, HotStore, ColdStore |
| `PageSize` | `crate::page` | crate root; `crate::layout::page` re-export | `buffer_pool`, `heap`, `btree`, manifest, HotStore, ColdStore, IO budgeting |
| `PageHeader` | `crate::page` | crate root; `crate::layout::page` re-export | `buffer_pool`, `heap`, `btree`, manifest, HotStore, ColdStore |
| `PageTrailer` | `crate::page` | crate root; `crate::layout::page` re-export | `buffer_pool`, `heap`, `btree`, manifest, HotStore, ColdStore |
| `PageLayoutContract` | `crate::page` | crate root; `crate::layout::page` re-export | `buffer_pool`, `heap`, `btree`, manifest, HotStore, ColdStore |
| `Lsn` | `crate::lsn` | crate root; WAL/page users import the same type | `buffer_pool`, `heap`, `btree`, WAL managers, recovery |
| `WalRecord`, `WalRecordHeader`, `WalRecordKind` | `crate::write_ahead_log::record` | `crate::write_ahead_log::*`, legacy `crate::wal::*`, crate root | `buffer_pool`, `heap`, `btree`, recovery, HADR, backup |

`buffer_pool`, `heap`, and `btree` must import these canonical types with `crate::...` paths
or crate-root re-exports. They must not define aliases, mirror structs, shadow enums, local
numeric tag tables, or "temporary" durable equivalents for `PageId`, `PageSize`,
`PageHeader`, `PageTrailer`, `PageLayoutContract`, `Lsn`, or `WalRecord`.

#### Future module responsibilities

- `buffer_pool` owns transient page residency, pin/guard lifetimes, dirty-page tracking,
  replacement policy, and WAL-before-page-flush coordination. It may define transient frame,
  guard, and eviction types, but persisted identity, page sizing/layout, and recovery ordering
  must use canonical `PageId`, `PageSize`, `PageHeader`, `PageTrailer`, `PageLayoutContract`,
  and `Lsn`.
- `heap` owns heap-page payload manipulation, slot directory helpers, free-space accounting,
  row locator helpers, and heap redo payload codecs. It must not define a heap-specific durable
  page header/trailer or a heap-specific WAL record envelope.
- `btree` owns access-path page payload manipulation, node-level helpers, key/child locator
  placement, and index redo payload codecs after explicit durable index page types exist. It
  must not overload existing heap page types, define a second page header/trailer, or define an
  index-specific WAL record envelope.

#### Duplicate-definition rejection rule

Any C2/N1/N2 patch that adds one of the following under `buffer_pool`, `heap`, `btree`, a
facade, or a test-only "format" module is invalid unless it is only a direct `pub use` of the
canonical owner:

- `PageId`
- `PageSize`
- `PageHeader`
- `PageTrailer`
- `PageLayoutContract`
- `Lsn`
- `WalRecord`, `WalRecordHeader`, or `WalRecordKind`

Payload-specific structures are allowed only when they describe bytes below the canonical
page header or inside canonical `WalRecord.payload`, and only after their serialization contract
is documented.

### 2. Page size policy

Andromeda V0 durable data pages support only the page sizes already represented by `PageSize`: **16 KiB** and **32 KiB**.

- **16 KiB is the default HotStore and heap page size** for C2/N1 unless a segment/allocation manifest explicitly chooses 32 KiB.
- **32 KiB is reserved for cold/large-page allocations and future analytical/index use**; it is valid only when recorded in the allocation/segment metadata and echoed by `PageHeader.page_size`.
- A physical page image is self-describing through `PageHeader.page_size`; readers must reject unsupported sizes.
- A single extent/segment must not mix page sizes unless a future DEC explicitly defines mixed-size extents.

### 3. PageHeader/PageTrailer reuse

All durable page images must reuse the existing `PageHeader` and `PageTrailer` contracts.

- No heap, B+Tree, manifest, HotStore, or ColdStore implementation may define an alternate durable header or trailer.
- `header_len` and `payload_offset` are the only allowed V0 extension points ahead of payload bytes.
- `PageTrailer` remains the common integrity boundary: payload CRC, page hash, and torn-write guard cover the payload image after header interpretation and before page acceptance.
- Header and trailer validation is required before a page is admitted into recovery, buffer-pool residency, HotStore reads, or ColdStore publication.

### 4. `page_lsn` rule

`PageHeader.page_lsn` is the highest durable WAL LSN whose redo effects are reflected in the page image.

- A page must not be flushed to HotStore or published to ColdStore until WAL is durable through at least `page_lsn` (**WAL-before-page-flush**).
- A transaction must not become visibly committed until its commit record is durable (**WAL-before-visible-commit**).
- Recovery treats pages with `page_lsn >= record.lsn` as already containing that record's redo effects.
- Page formatting/allocation records own the initial nonzero `page_lsn`; free pages still carry a valid nonzero `page_lsn` for recovery ordering.
- RAM is never truth: dirty buffer contents are not durable state until the WAL and page-image ordering above is satisfied.

### 5. Heap slot directory policy

Heap pages use a slotted-page payload layout under `PageType::FixedRow` or `PageType::HybridRow`.

- The slot directory lives inside the page payload and grows upward from `payload_offset`.
- Row cells grow downward from the payload end (`payload_offset + payload_len`) toward the slot directory.
- `free_start`, `free_end`, and `free_bytes` describe the free gap between the directory and row-cell area.
- `slot_count` is the number of allocated slot entries, including deleted/reusable entries.
- `row_count` is the number of live logical rows and must never exceed `slot_count`.
- Slot identifiers are stable within a page. Compaction may move row bytes but must update the slot entry; WAL redo must address row changes by page identity plus slot identity, not by unstable byte offset alone.
- Deleted slots become tombstones/reusable entries; physical removal that renumbers visible slots is forbidden in V0.

### 6. Row payload encoding boundary

The page layer owns placement, free-space accounting, slot metadata, page integrity checks, and page-level MVCC/recovery ordering. It does **not** own schema-aware row encoding.

- Row payload bytes stored in row cells are canonical bytes produced by the row/binary-format layer.
- The page layer treats row payloads as opaque bytes except for length, slot state, and any page-owned MVCC locator metadata explicitly defined by implementation contracts.
- WAL row redo payloads must carry enough canonical row bytes and locator metadata to reconstruct the page effect without consulting RAM-only state.
- Any change to canonical row value encoding belongs to the binary-format decision stream and must remain compatible with WAL redo semantics.

### 7. B+Tree node layout assumptions

B+Tree pages must reuse `PageHeader` and `PageTrailer`, but durable B+Tree code is deferred until explicit page types are added.

- Implementations must not overload `FixedRow`, `HybridRow`, `Manifest`, or `Free` for B+Tree nodes.
- Before durable B+Tree pages land, the storage crate must add explicit page-type variants for index internal and index leaf pages, or record an equivalent DEC addendum.
- B+Tree node payloads will use a small node header followed by a slotted variable-length entry area.
- Internal entries map separator keys to child `PageId`s; leaf entries map keys to row locators/version locators.
- Sibling/high-key metadata belongs in the B+Tree node payload unless promoted into `PageHeader.previous_page_id`/`next_page_id` by a later accepted decision.
- Key encoding is owned by the index-key/binary-format layer; page code owns node placement and redo-safe structural metadata only.

### 8. Page type and versioning policy

`PageHeader.format_version` is the durable page-format version. V0 is represented by value `1`.

- Readers must reject unsupported page format versions and unknown page types.
- Structural changes to header/trailer interpretation, slot-entry semantics, or mandatory payload layout require a page-format version bump or a DEC addendum proving backward-compatible handling.
- Adding a new durable page type requires stable numeric tags in the binary-format layer before any on-disk image is emitted.
- `header_len` may increase within the same format only when older readers can safely reject or skip by contract; otherwise the format version must advance.
- `PageType::Free` pages may not advertise rows or slots and must not be used as a generic "unknown" escape hatch.

### 9. WAL payload ownership

WAL frame structure owns record ordering, frame checksum validation, LSN chaining, and payload length. Storage-format code owns the semantic payload bytes for page/row/index redo records.

- WAL payload definitions for `PageAllocate`, `PageFormat`, `RowInsert`, `RowUpdate`, `RowDelete`, `IndexInsert`, and `IndexDelete` must be documented before those records become durable.
- Every page-affecting WAL payload must identify the target object/allocation/page and the redo operation's logical locator, and must be idempotent when compared with page `page_lsn`.
- WAL payloads must not depend on buffer-pool addresses, process memory, GPU state, or hidden mutable globals.
- Manifest switching remains WAL-owned at the record-ordering boundary and storage-owned at the immutable publication boundary.
- ColdStore publication is immutable: once a cold page/segment is published, it is never mutated in place; later changes require new page/segment images plus manifest switching.

---

## Preserved invariants

- RAM is never truth.
- WAL-before-page-flush and WAL-before-visible-commit are mandatory.
- The canonical recoverable state is the last valid cold snapshot plus durable WAL.
- ColdStore publication is immutable.
- No unsafe code is required or permitted for durable storage-format handling.
- GPU participation is forbidden in commit, WAL, rollback, recovery, page flush, or page publication paths.

---

## Explicit deferrals

- Byte-for-byte page header/trailer serialization table, numeric enum tags, and golden vectors.
- Exact heap slot-entry byte width and flag-bit assignments.
- Canonical row value/type encoding.
- Durable B+Tree page-type enum additions and node-header byte layout.
- Buffer pool, heap, and B+Tree implementation code.
- Compression, encryption, prefix compression, page checksumming algorithm changes, and page-level MVCC compaction policy.
- Recovery redo/undo algorithm details beyond the `page_lsn` idempotence and WAL ordering rules stated here.
- Any decision to expose `buffer_pool`, `heap`, or `btree` as public module paths instead of
  crate-root re-exported APIs.

---

## Validation criteria

- `docs/decisions` contains no existing `DEC-032` record before this file.
- Page implementation work references this DEC before emitting durable page images.
- Code continues to use `crate::page` as the canonical page contract and `layout/page.rs` only as a re-export facade.
- `buffer_pool`, `heap`, and `btree` skeleton work declares modules as crate-private unless a
  later accepted DEC creates a public facade.
- Reviews reject duplicate definitions or aliases of `PageId`, `PageHeader`, `PageTrailer`,
  `Lsn`, `WalRecord`, `WalRecordHeader`, and `WalRecordKind` outside their canonical owners.
- Storage WAL payload implementation adds documentation/tests for each durable page-affecting record before enabling writes.
- Doctrine scans for unsafe code and GPU critical-path drift remain clean.

