# BufferPoolPolicy v0 Specification

## Purpose

Define the accepted roadmap contract for Andromeda buffer-pool residency,
pinning, dirty tracking, WAL durability cooperation, checkpoint flush ordering,
and read-ahead policy.

The buffer pool is transient runtime state. It is not durable database truth,
not a page byte format, not a WAL format, and not a transaction visibility
owner. It must cooperate with WAL, checkpoint, and recovery components without
bypassing the rule that durable WAL precedes visible commit and page flush.

## Scope

This specification applies to the current `andromeda-storage` buffer-pool
surface:

- `BufferPoolConfig`;
- `BufferPool`;
- `BufferPoolManager`;
- `BufferFrame`;
- `PageGuard` and `PageGuardMut`;
- `DirtyTracker`;
- `FlushAllDirtyResult`;
- `WalDurabilityObserver`;
- `PageStore`.

It covers:

- pin and unpin lifecycle;
- dirty page LSN tracking;
- WAL durability gating before page flush;
- checkpoint-oriented flush candidate ordering;
- read-ahead as an advisory optimization;
- the boundary between buffer-pool flushing and transaction visibility.

## Current Implementation Status

The Rust workspace currently implements:

- fixed-capacity buffer-pool residency over a `PageStore`;
- transient `BufferFrameId` identities;
- RAII page guards that release pins on drop;
- explicit pin and unpin through `BufferPoolManager`;
- dirty tracking by page id, first dirty LSN, and latest dirty LSN;
- deterministic flush candidates ordered by first dirty LSN and page id;
- `WalDurabilityObserver` and `flush_all_dirty_with_report`;
- explicit blocked-state reporting when durable WAL does not cover the latest
  dirty LSN;
- legacy `flush_all_dirty` rejection when dirty pages exist without a WAL
  durability observer.

Missing implementation owner: checkpoint scheduler. The current buffer pool can
produce ordered dirty candidates and flush eligible pages, but it does not own
checkpoint begin/end WAL records, checkpoint epoch publication, or recovery
floor advancement.

Missing implementation owner: read-ahead scheduler. The current buffer pool has
no public read-ahead API. Any future read-ahead implementation must remain
advisory and must not become a source of commit visibility, recovery truth, or
dirty state.

Missing implementation owner: commit visibility coordinator. The current buffer
pool does not make transactions visible. It can only enforce
WAL-before-page-flush and report blocked dirty pages.

## Non-goals

This specification does not:

- introduce application-facing SQL, dynamic command text, gRPC, or runtime JSON
  defaults;
- bypass typed Procedure contracts;
- define a page, WAL, manifest, heap, B-Tree, or segment byte layout;
- serialize Rust native structs to disk or network;
- make RAM, buffer-pool state, read-ahead output, GPU output, benchmark output,
  or temp storage a source of database truth;
- define a complete checkpoint implementation;
- define transaction commit publication or MVCC visibility rules;
- require read-ahead for correctness.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for Andromeda storage, WAL, recovery, and critical-path
  invariants.
- `crates/AGENTS.md` for Rust crate boundaries.
- `documentations/specs/PageHeader_PageTrailer_v0.md` for page identity,
  page LSN, and WAL relationship.
- `documentations/specs/WalRecord_v0.md` for durable WAL and visible commit
  ordering.
- `documentations/specs/PageLifecycle_v0.md` for the page lifecycle roadmap
  contract.
- `crates/andromeda-storage/src/buffer_pool/mod.rs` for the public buffer-pool
  module contract.
- `crates/andromeda-storage/src/buffer_pool/manager.rs` for current
  `BufferPool` and `BufferPoolManager` behavior.
- `crates/andromeda-storage/src/buffer_pool/frame.rs` for pin, dirty, flush,
  and eviction state transitions.
- `crates/andromeda-storage/src/buffer_pool/dirty.rs` for dirty candidate
  ordering.
- `crates/andromeda-storage/src/buffer_pool/wal_durability.rs` for the WAL
  observer contract.
- `crates/andromeda-storage/src/buffer_pool/manager/flush.rs` for dirty flush
  reporting.
- `crates/andromeda-storage/src/page/store.rs` for `PageStore` WAL-before-page
  flush preconditions.

## Procedure

### Ownership

`andromeda-storage::buffer_pool` owns transient residency metadata:

- frame table membership;
- pins;
- dirty metadata;
- clock replacement state;
- flush eligibility reporting.

The buffer pool must import durable identity and ordering from canonical owners:

- `PageId`, `PageSize`, `PageHeader`, `PageTrailer`, and
  `PageLayoutContract` from the page module;
- `Lsn` from the WAL-owned LSN model re-exported by storage;
- durable WAL coverage from a `WalDurabilityObserver`;
- page bytes through explicit page image and page store APIs.

The buffer pool must never persist `BufferFrameId`, pin count, clock usage,
dirty tracker maps, read-ahead hints, or guard state.

### Pin and unpin lifecycle

Pinning is a transient exclusion contract. A pinned resident frame:

- must not be evicted;
- must not be flushed while its pin count is nonzero;
- must keep a nonzero, validated page identity and page LSN;
- may be dirtied only with a nonzero dirty LSN that is greater than or equal to
  the resident page LSN.

Unpinning releases the transient exclusion. It does not make a dirty page
durable, visible, or clean.

Required rules:

| Area | Rule |
| --- | --- |
| Pin identity | `BufferFrameId` is transient and must not appear in WAL, page bytes, manifests, or segment indexes. |
| Pin increment | Pinning a resident frame increments a bounded pin count or fails closed. |
| Pin underflow | Unpinning an unpinned frame fails closed. |
| Guard drop | RAII guards release an acquired pin exactly once. |
| Eviction | A pinned frame is not reusable for another page. |
| Flush | A pinned dirty frame is reported as pinned and remains dirty. |
| Dirty mutation | Dirty mutation requires a current pin. |

### Dirty tracking

Dirty tracking records the recovery range for a resident page. It is not the
source of durable data.

For each dirty page, the tracker must keep:

- `page_id`;
- `first_dirty_lsn`;
- `last_dirty_lsn`.

Repeated dirty marks for the same page must not create duplicate dirty records.
The first dirty LSN is used for scheduling fairness and checkpoint progress.
The latest dirty LSN is the WAL durability fence for flush.

Dirty LSN rules:

- dirty LSN zero is invalid;
- a dirty LSN earlier than the resident page LSN is invalid;
- a frame must not regress from a later dirty LSN to an earlier dirty LSN;
- a flush may mark a page clean only after the flushed LSN covers the latest
  dirty LSN;
- failed or blocked flush attempts must leave the page dirty and retryable.

### WAL durability cooperation

The buffer pool must not flush a dirty page unless durable WAL covers the page's
latest dirty LSN.

The current contract is:

1. Collect dirty flush candidates from `DirtyTracker`.
2. For each candidate, read the latest dirty LSN.
3. Ask `WalDurabilityObserver` for the maximum durable WAL LSN.
4. Reject or defer the flush when `last_dirty_lsn > max_durable_lsn`.
5. Report the blocked page through `FlushAllDirtyResult`.
6. Preserve dirty state for retry.
7. Write the page through `PageStore` only after the WAL durability fence is
   satisfied.
8. Mark the frame and dirty tracker clean only after the page store write
   succeeds.

The legacy `BufferPoolManager::flush_all_dirty` path must reject dirty pages
because it does not receive a WAL durability observer.

### Checkpoint flush ordering

Checkpoint flush ordering is a storage and recovery contract. The buffer pool
provides the ordered dirty candidate primitive, but the checkpoint scheduler is
not implemented in this module.

Required checkpoint behavior:

1. Append and durably flush the checkpoint begin WAL record before checkpoint
   state is advertised.
2. Enumerate dirty pages in deterministic order by oldest `first_dirty_lsn`,
   then by `PageId`.
3. Flush only pages whose latest dirty LSN is covered by durable WAL.
4. Keep blocked pages dirty and report their LSN gaps.
5. Do not advance the checkpoint recovery floor past a dirty page whose latest
   dirty LSN is not durable and flushed or otherwise covered by accepted
   checkpoint policy.
6. Append and durably flush the checkpoint end WAL record only after the
   checkpoint state is complete and recoverable.
7. Publish checkpoint metadata only after its WAL evidence is durable.

The buffer pool must expose enough report data for checkpoint code to decide
whether progress was complete, partial, or blocked. It must not silently drop a
dirty page from the checkpoint set.

### Read-ahead policy

Read-ahead is advisory only.

A future read-ahead scheduler may:

- request likely next pages;
- validate page layout before admission;
- admit clean, unpinned pages into the buffer pool when capacity and policy
  allow;
- record metrics or traces about useful and wasted prefetches;
- cancel read-ahead when pressure, recovery, shutdown, or WAL priority requires
  it.

A read-ahead scheduler must not:

- mark pages dirty;
- create user-visible pins;
- bypass page format validation;
- bypass WAL durability checks;
- expose commit visibility;
- block transaction commit, rollback, WAL flush, recovery, catalog publication,
  security, or checkpoint correctness;
- treat speculative page residency as database truth.

When read-ahead is absent or disabled, demand reads, WAL replay, checkpoint
flush, and transaction visibility must remain correct.

### No visible commit before durable WAL

The buffer pool is not the commit visibility coordinator. However, it must
preserve the storage side of the invariant:

- dirty page flush must wait for durable WAL coverage;
- a blocked dirty page must remain dirty;
- a failed page-store write must remain retryable;
- flush success must not imply transaction visibility by itself.

The commit visibility owner must publish transaction effects only after the
commit WAL record and all required mutation WAL records are durable. Page flush
may occur before or after visibility only when WAL durability and recovery
rules are preserved.

## Validation

Documentation acceptance checks:

- The spec states that buffer-pool state is transient and not durable truth.
- The spec preserves WAL-before-page-flush and WAL-before-visible-commit.
- The spec distinguishes first dirty LSN scheduling from latest dirty LSN
  durability.
- The spec marks checkpoint, read-ahead, and commit visibility owners as
  missing where current code does not implement them.
- The spec does not introduce SQL, gRPC, runtime JSON, native struct layout
  persistence, or GPU critical-path behavior.

Existing code evidence to use when code validation is allowed:

```powershell
cargo test -p andromeda-storage --test buffer_pool_policy_contract
cargo test -p andromeda-storage --test buffer_pool_wal_fence_contract
cargo test -p andromeda-storage --test buffer_pool_module_exports
```

### Validation matrix

| Area | Positive case | Negative case | Required outcome |
| --- | --- | --- | --- |
| Pin | Resident clean page is pinned. | Nonresident or invalid frame is pinned. | Succeed for resident page; fail closed otherwise. |
| Unpin | Pinned frame is unpinned once. | Unpinned frame is unpinned again. | Release once; reject underflow. |
| Eviction | Clean unpinned frame is reusable. | Pinned or dirty frame is selected. | Reuse clean unpinned frame only. |
| Dirty mark | Pinned page is marked with nonzero LSN greater than or equal to page LSN. | Zero, regressing, or pre-page dirty LSN is used. | Track valid dirty range; reject invalid LSN. |
| Dirty duplicate | Same page is dirtied repeatedly. | Duplicate dirty records appear. | Keep one entry with first and latest dirty LSNs. |
| Flush WAL fence | Latest dirty LSN is durable. | Latest dirty LSN is ahead of durable WAL. | Flush eligible page; report blocked page and keep it dirty. |
| Pinned flush | Dirty frame has pin count zero. | Dirty frame is still pinned. | Flush only unpinned frame; report pinned error otherwise. |
| Checkpoint order | Dirty candidates are ordered by first dirty LSN, then page id. | Flush order depends on map insertion or frame id. | Use deterministic LSN/page ordering. |
| Read-ahead | Prefetched clean page is validated and optional. | Read-ahead marks dirty or affects visibility. | Allow only advisory clean prefetch; reject correctness dependency. |
| Visible commit | Commit WAL is durable before publication. | Commit is visible before durable WAL. | Publish only after durable WAL evidence. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Dirty page flushes while WAL is behind. | `WalDurabilityObserver` was bypassed or the wrong LSN was checked. | Gate flush on the latest dirty LSN and preserve blocked dirty state. |
| Checkpoint skips a dirty page. | The checkpoint scheduler treated partial flush as complete. | Use `FlushAllDirtyResult` and keep blocked or errored pages in the checkpoint decision. |
| Pinned frame is evicted. | Pin count was ignored by replacement policy. | Reject eviction for any nonzero pin count. |
| Page is marked dirty without a pin. | Mutation path bypassed guard or explicit pin lifecycle. | Require a live pin before dirty mutation. |
| Read-ahead changes transaction results. | Speculative residency was treated as truth. | Make read-ahead clean, cancelable, and advisory only. |
| Visible commit depends on page flush success. | Commit visibility and page durability were conflated. | Use durable WAL as the visibility prerequisite; page flush remains recovery optimization. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `documentations/specs/PageHeader_PageTrailer_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/PageLifecycle_v0.md`
- `crates/andromeda-storage/src/buffer_pool/mod.rs`
- `crates/andromeda-storage/src/buffer_pool/manager.rs`
- `crates/andromeda-storage/src/buffer_pool/frame.rs`
- `crates/andromeda-storage/src/buffer_pool/guard.rs`
- `crates/andromeda-storage/src/buffer_pool/dirty.rs`
- `crates/andromeda-storage/src/buffer_pool/wal_durability.rs`
- `crates/andromeda-storage/src/buffer_pool/flush_result.rs`
- `crates/andromeda-storage/src/buffer_pool/manager/flush.rs`
- `crates/andromeda-storage/src/page/store.rs`
- `crates/andromeda-storage/tests/buffer_pool_wal_fence_contract.rs`
