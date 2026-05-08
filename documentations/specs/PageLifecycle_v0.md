# PageLifecycle v0 Specification

## Purpose

Define the accepted roadmap contract for Andromeda page lifecycle behavior from
allocation through residency, mutation, dirty tracking, WAL durability, flush,
checkpoint participation, eviction, and recovery.

A page lifecycle is valid only when durable page bytes, WAL evidence, and
visibility decisions remain owned by their canonical subsystems. A resident
page in RAM is never database truth by itself.

## Scope

This specification applies to the current `andromeda-storage` page and
buffer-pool lifecycle surface:

- `PageId`, `ObjectId`, and `AllocationId`;
- `PageSize`, `PageType`, and `PageFlags`;
- `PageHeader`, `PageTrailer`, and `PageLayoutContract`;
- `PageImage`;
- `PageStore` and `InMemoryPageStore`;
- `BufferPool`, `BufferFrame`, `PageGuard`, `PageGuardMut`, and
  `DirtyTracker`;
- `WalDurabilityObserver` for storage flush cooperation.

It covers:

- page identity and layout validation;
- allocation and resident admission;
- pin and unpin lifecycle;
- dirty LSN tracking;
- WAL durability cooperation;
- checkpoint flush ordering;
- read-ahead as advisory;
- no visible commit before durable WAL.

## Current Implementation Status

The Rust workspace currently implements:

- canonical page identity and layout domain models;
- page layout validation for nonzero identities, nonzero page LSN, page links,
  free-space bounds, row/slot consistency, flags, and trailer evidence;
- exact page image length validation for 16 KiB and 32 KiB pages;
- `InMemoryPageStore` validation that page writes require prior allocation and
  durable WAL coverage at least through the image page LSN;
- buffer-pool admission through validated page images;
- transient pin, dirty, flush, and eviction state transitions;
- dirty flush reporting with WAL durability blocking.

Missing implementation owner: read-ahead scheduler. There is no public
read-ahead API. Read-ahead remains a future advisory optimization only.

Missing implementation owner: checkpoint scheduler. Current code exposes
ordered dirty candidates and flush reports, but does not own checkpoint
begin/end WAL records or recovery floor publication.

Missing implementation owner: commit visibility coordinator. Current storage
code enforces WAL-before-page-flush, but it does not publish transaction
visibility.

## Non-goals

This specification does not:

- introduce application-facing SQL, dynamic command text, gRPC, or runtime JSON
  defaults;
- bypass typed Procedure contracts;
- define a new page, WAL, manifest, heap, B-Tree, or segment byte layout;
- serialize Rust native structs to disk or network;
- make RAM, read-ahead output, buffer-pool residency, temp storage, GPU output,
  or benchmark output database truth;
- define the complete transaction kernel or MVCC visibility protocol;
- define the complete checkpoint implementation;
- require read-ahead for correctness.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for Andromeda mission-critical invariants.
- `crates/AGENTS.md` for Rust crate boundaries.
- `documentations/specs/PageHeader_PageTrailer_v0.md` for current page
  header, trailer, page LSN, and page integrity requirements.
- `documentations/specs/WalRecord_v0.md` for WAL durability and visible commit
  ordering.
- `documentations/specs/BufferPoolPolicy_v0.md` for residency, pinning,
  dirty tracking, and flush policy.
- `crates/andromeda-storage/src/page/layout.rs` for `PageLayoutContract`
  validation.
- `crates/andromeda-storage/src/page/image.rs` for page image length and
  layout attachment.
- `crates/andromeda-storage/src/page/store.rs` for current page-store
  preconditions.
- `crates/andromeda-storage/src/buffer_pool/frame.rs` for resident lifecycle
  state transitions.
- `crates/andromeda-storage/src/buffer_pool/manager/flush.rs` for WAL-gated
  dirty flush.

## Procedure

### Ownership

The page module owns durable page identity and page layout contracts. The
buffer pool owns only transient residency. WAL owns durability ordering.
Transaction and MVCC components own visibility.

No subsystem may derive durable page bytes from Rust native struct memory
layout. Persistent page bytes must use explicit codecs and must be validated
before admission as accepted page state.

### Lifecycle states

The lifecycle uses these conceptual states:

| State | Meaning | Persistence rule |
| --- | --- | --- |
| Allocated | Page identity and layout contract exist in a page store. | Allocation must have WAL evidence before it can become visible database state. |
| Resident clean | Validated page image is loaded in a buffer frame with no dirty LSN. | RAM copy is cache state only. |
| Pinned | Resident frame has a nonzero pin count. | Prevents eviction and flush while pinned. |
| Resident dirty | Pinned mutation recorded a nonzero dirty LSN. | Must remain dirty until page flush succeeds after WAL durability. |
| WAL-durable dirty | Latest dirty LSN is covered by durable WAL. | Eligible for page flush when unpinned. |
| Flushing | Unpinned dirty page is being written through `PageStore`. | Dirty state remains until the write succeeds. |
| Resident clean after flush | Page write succeeded and dirty metadata was cleared. | Recovery still relies on WAL and accepted page bytes. |
| Evicted | Clean unpinned page left the buffer pool. | Durable store remains the source for future demand reads. |
| Recovered | Startup reconstructed accepted state from validated storage plus durable WAL. | Unknown or corrupt formats fail closed except read-only forensic paths. |

### Page identity and layout validation

A page lifecycle cannot start from an unvalidated or anonymous durable page.

Required page layout rules:

- page id must be nonzero;
- non-free pages must carry nonzero object and allocation ids;
- page LSN must be nonzero;
- page epoch must be nonzero;
- page links must not point to the page itself;
- link presence must match page flags;
- unknown page flags must be rejected;
- header length, payload offset, payload length, free-space offsets, and
  trailer placement must be bounded by page size;
- row count must not exceed slot count;
- header CRC, payload CRC, payload hash, and torn-write guard evidence must be
  nonzero;
- torn-write guard must not be only the page id.

### Allocation and admission

Allocation creates or attaches a validated `PageImage` through a `PageStore`.
Resident admission loads that image into a buffer frame only after layout
validation succeeds.

The current `InMemoryPageStore` requires a caller-supplied durable LSN for
allocation and write. It rejects page writes when the durable LSN is behind the
image page LSN.

Future disk-backed allocation must preserve this ordering:

1. Produce the WAL record that describes the allocation or mutation.
2. Make the relevant WAL range durable.
3. Admit or publish the page state only through validated page images.
4. Flush page bytes only when durable WAL covers the page LSN or latest dirty
   LSN required by the flush policy.

### Pin, mutation, and unpin

Page mutation requires a live pin. A caller may read a resident page through an
immutable guard or mutate through a mutable guard.

Mutation rules:

- dirty LSN must be nonzero;
- dirty LSN must be greater than or equal to the page LSN;
- the first dirty LSN remains the scheduling floor;
- the latest dirty LSN advances the WAL durability fence;
- unpin does not make dirty data durable or visible;
- a dropped guard releases its pin exactly once.

### WAL durability and page flush

No dirty page may be flushed unless durable WAL covers its latest dirty LSN.

The page lifecycle must preserve these outcomes:

- WAL lag: report the page as blocked and keep it dirty.
- Pinned page: report the page as pinned and keep it dirty.
- Page-store write failure: report an error and keep the page dirty.
- Successful flush: clear frame dirty state and dirty tracker state only after
  the page-store write succeeds.

Flush success is not commit visibility. It is one durable storage action that
must still be interpreted by recovery and transaction visibility rules.

### Checkpoint participation

A checkpoint may use buffer-pool dirty candidates, but checkpoint ownership is
separate from the page module and buffer pool.

Required checkpoint page lifecycle rules:

1. Checkpoint begin must have durable WAL evidence before checkpoint progress is
   advertised.
2. Dirty page candidates must be processed in deterministic order by first
   dirty LSN, then page id.
3. A page whose latest dirty LSN is not durable must remain dirty and must not
   be counted as flushed checkpoint state.
4. Checkpoint end and recovery floor publication must wait for complete,
   recoverable evidence.
5. A crash during checkpoint must leave recovery able to choose the previous
   valid recovery floor or a fully validated new one.

### Read-ahead policy

Read-ahead is advisory only.

When a future read-ahead scheduler exists, it may prefetch likely pages into
clean, unpinned residency after validating page format. It must be cancelable
and bounded by buffer pressure, WAL priority, checkpoint pressure, shutdown,
and recovery mode.

Read-ahead must not:

- allocate pages;
- mark pages dirty;
- create user-visible pins;
- make transaction effects visible;
- advance checkpoint state;
- bypass WAL durability;
- block commit, rollback, WAL flush, recovery, catalog publication, or
  security-critical paths;
- become required for correctness.

When read-ahead is disabled or absent, demand reads and recovery must remain
correct. A demand read of a missing page must not allocate or dirty a page as a
side effect.

### No visible commit before durable WAL

No visible commit before durable WAL is a cross-subsystem invariant.

The page lifecycle contributes these storage-side rules:

- page allocation, mutation, and flush must be tied to nonzero LSN evidence;
- dirty flush must wait for durable WAL coverage;
- buffer-pool dirty state must not be treated as commit visibility;
- page-store write success must not publish transaction effects by itself.

The commit visibility coordinator must publish transaction effects only after
the commit WAL record and all required mutation WAL records are durable.

## Validation

Documentation acceptance checks:

- The spec states that page and buffer-pool RAM state is not database truth.
- The spec requires nonzero page LSNs and explicit page layout validation.
- The spec preserves WAL-before-page-flush and WAL-before-visible-commit.
- The spec states that read-ahead is advisory and currently has no owner.
- The spec marks checkpoint and commit visibility owners as missing where
  current code does not implement them.
- The spec does not introduce SQL, gRPC, runtime JSON, native struct layout
  persistence, or GPU critical-path behavior.

Existing code evidence to use when code validation is allowed:

```powershell
cargo test -p andromeda-storage --test page_lifecycle_contract
cargo test -p andromeda-storage --test property_page_codec_v1
cargo test -p andromeda-storage --test property_page_parsing
cargo test -p andromeda-storage --test buffer_pool_wal_fence_contract
```

### Validation matrix

| Area | Positive case | Negative case | Required outcome |
| --- | --- | --- | --- |
| Page LSN | Nonzero page LSN. | Zero page LSN. | Accept valid page; reject zero LSN before residency. |
| Page image | Image length equals page size and layout validates. | Wrong length or invalid layout. | Accept only exact validated image. |
| PageStore allocation | New page id with valid layout and sufficient durable LSN. | Duplicate page id or WAL LSN behind page LSN. | Allocate valid page; reject invalid request. |
| Demand read | Existing page id is read. | Missing page id is read. | Return page or `None`; do not allocate or dirty. |
| Pin | Resident page is pinned. | Pinned frame is evicted or flushed. | Prevent eviction/flush until unpinned. |
| Dirty | Pinned page is dirtied with valid LSN. | Dirty LSN is zero, regresses, or predates page LSN. | Track valid dirty range; reject invalid dirty mark. |
| Flush | Latest dirty LSN is durable and page is unpinned. | WAL is behind, frame is pinned, or write fails. | Flush valid page; otherwise report and keep dirty. |
| Checkpoint | Ordered candidates are flushed with durable WAL coverage. | Candidate is skipped or checkpoint floor advances past blocked dirty page. | Preserve deterministic order and fail closed on incomplete evidence. |
| Read-ahead | Clean validated page is prefetched opportunistically. | Prefetch affects visibility or dirty state. | Permit only advisory clean prefetch. |
| Commit | Commit WAL is durable before visibility. | Commit visible before durable WAL. | Reject publication until durable WAL evidence exists. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Page with LSN zero becomes resident. | Page layout validation was bypassed. | Require `PageLayoutContract::validate` before image admission. |
| Demand read creates a page. | Read path was confused with allocation or read-ahead. | Keep demand read side-effect free for missing pages. |
| Dirty page becomes clean after WAL-lagged flush. | Flush code cleared dirty state before durable WAL or successful page write. | Keep dirty state until both WAL fence and page-store write succeed. |
| Checkpoint recovery floor advances past dirty pages. | Partial checkpoint progress was treated as complete. | Use ordered dirty candidates and blocked/error reports before publication. |
| Read-ahead blocks WAL flush or recovery. | Advisory prefetch was placed on a critical path. | Cancel or deprioritize read-ahead behind WAL, recovery, checkpoint, and shutdown work. |
| Transaction effects appear before commit WAL durability. | Commit visibility coordinator ignored the WAL fence. | Publish only after durable commit and mutation WAL evidence. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `documentations/specs/PageHeader_PageTrailer_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/BufferPoolPolicy_v0.md`
- `crates/andromeda-storage/src/page/layout.rs`
- `crates/andromeda-storage/src/page/image.rs`
- `crates/andromeda-storage/src/page/store.rs`
- `crates/andromeda-storage/src/buffer_pool/frame.rs`
- `crates/andromeda-storage/src/buffer_pool/guard.rs`
- `crates/andromeda-storage/src/buffer_pool/dirty.rs`
- `crates/andromeda-storage/src/buffer_pool/wal_durability.rs`
- `crates/andromeda-storage/src/buffer_pool/manager/flush.rs`
- `crates/andromeda-storage/tests/page_lifecycle_contract.rs`
- `crates/andromeda-storage/tests/buffer_pool_policy_contract.rs`
