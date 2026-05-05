# DEC-034: B+ Tree Index Engine Design Record

**Status:** ACCEPTED (Design Phase)  
**Version:** 1.0  
**Date:** Q2 2026  
**Owner:** Storage Engine Architect  
**Task:** N2-BTREE-IDX-001 — Design B-Tree Index Engine  
**Supersedes:** None  
**Related:** DEC-032 (Storage Durable Page Format)  

---

## Context

Andromeda has Wave 17 storage foundations (BufferPool, Heap Page Engine, WAL) ready for indexing. The B+ Tree Index Engine is required for:

1. **Efficient key-value lookup** — O(log N) instead of O(N) table scans
2. **Range queries** — Sorted leaf-node access for SQL range predicates
3. **Uniqueness enforcement** — Unique index constraints
4. **Crash recovery** — Index rebuild from heap + WAL redo

This DEC specifies the architecture and invariants for B+ Tree implementation in Wave 18+. It is a **design-only** document; implementation is explicitly deferred to Wave 18.

### Facts

- **Wave 17 delivered:** BufferPool with page caching, Heap Page Engine with slotted pages, WAL with durability ordering
- **DEC-032 mandates:** All index pages must use standard PageHeader/PageTrailer; no custom headers allowed
- **Recovery contract:** Indexes must be rebuildable from table scan + LSN redo; transient index corruption is acceptable if recovery rebuilds
- **Branching factor tuning:** Larger branching factors (128+) reduce tree height but increase node CPU cost
- **Key format:** Composite keys are variable-length byte sequences, serialized with collation-aware ordering
- **Concurrency requirement:** Multiple concurrent index operations (lookup + insert) must not deadlock

### Assumptions for This Design

1. **RowId representation:** 64-bit identifier (page_id + slot_id or similar); defined in catalog/storage domain
2. **Page sizes:** 16 KiB and 32 KiB (from DEC-032); branching factor tuned per page size
3. **Latch-based concurrency:** No latch-free data structure complexity in Wave 18; latches via BufferPool guards
4. **Index rebuild time:** Acceptable for Wave 18+ (seconds to minutes for large tables); future bulk-load optimization in Wave 19
5. **Null handling:** NULLs are allowed in indexes; NULL < all non-NULL keys; multiple NULLs permitted even in unique indexes
6. **Crash behavior:** Tree corruption post-crash is acceptable; recovery detects and rebuilds

---

## Decision

### 1. B+ Tree Structure Invariants

**All implementations must enforce:**

```
Inv-1: Leaf Balance
  All leaf nodes are at the same tree depth.
  (Maintained by split/merge logic to preserve balance)

Inv-2: Key Ordering
  Within each node: key[0] < key[1] < ... < key[n-1]
  Across siblings: max_key(left_sibling) < min_key(right_sibling)

Inv-3: Child Pointer Invariant (Internal Nodes)
  For keys k0, k1, ..., kn-1 and children c0, c1, ..., cn:
    All keys in c0 < k0
    All keys in c[i] in [k[i-1], k[i])  for 0 < i < n
    All keys in c[n] >= k[n-1]

Inv-4: Occupancy
  Node.key_count >= branching_factor / 2  (except root, which may have 1 key)
  Node.key_count <= branching_factor - 1
  (Violations trigger split/merge on next operation)

Inv-5: Leaf Sibling Chain
  Leaf nodes are linked: L.next_leaf -> R (left to right in key order)
  Last leaf: L.next_leaf = None (or page_id 0)

Inv-6: Key Completeness
  Every key in the tree is present in exactly one leaf node.
  Every leaf key is reachable from root via child pointer chain.

Inv-7: Unique Constraint
  If index.is_unique:
    For each leaf key K: row_count(K) == 1
    Enforcement: Insert rejects duplicate non-NULL keys
    (Multiple NULLs are permitted per SQL standard)
```

**Violation Detection:**
- Recovery detects leaf-balance violations (height mismatch) → triggers rebuild
- BufferPool validates PageHeader/PageTrailer → detects torn writes
- Startup consistency check DFS traversal → detects structural corruption

### 2. Node Layout and Serialization

**Node Header (24 bytes, common to internal and leaf):**

```
┌──────────────────────────┬────────┬────────┐
│ Field                    │ Type   │ Bytes  │
├──────────────────────────┼────────┼────────┤
│ node_id                  │ PageId │ 8      │
│ is_leaf (bool)           │ u8     │ 1      │
│ key_count                │ u16    │ 2      │
│ parent_id / next_leaf_id │ PageId │ 8      │
│ reserved                 │        │ 5      │
└──────────────────────────┴────────┴────────┘
```

- **Internal nodes:** parent_id points to parent node
- **Leaf nodes:** parent_id field repurposed as next_leaf_id (sibling link)
- **Padding:** 5 bytes reserved for future expansion (e.g., LSN, flags)

**Payload Format:**

```
Internal Node:
  Header (24) | Keys (variable) | Child Pointers (8 * (key_count + 1))
  
Leaf Node:
  Header (24) | Keys (variable) | Row IDs (8 * key_count or variable for non-unique)
```

**Key Serialization (Variable-Length):**

```
[Length: u16 BE] [Key Data] [NULL Bitmap (if needed)]

Examples:
  key="alice"     → [0x05] "alice"
  key=12345       → [0x08] [0x00 0x00 0x00 0x00 0x00 0x00 0x30 0x39]
  key=NULL        → [0xFF 0xFF] (special NULL marker)
  key=(12, NULL)  → [len] [12 bytes] [NULL bitmap=0x02]
```

**Serialization Rules:**
- Keys must be comparable byte-by-byte (big-endian for integers)
- NULL keys sort before non-NULL keys
- Composite keys: serialize columns in order, track NULL bitmap

**Wave 18 Implementation:**
- Serialization code lives in `btree.rs`; format owned by storage, not binary-format module
- PageHeader/PageTrailer validation performed by BufferPool before node deserialization
- Format versioning deferred; V0 format is fixed for this DEC

### 3. Core Operations and Algorithms

#### 3.1 Lookup(key: &[u8]) → Vec<RowId>

```
1. Start at root
2. While node is internal:
     - Binary search keys to find child index
     - Load child via BufferPool
3. At leaf:
     - Binary search for key
     - Return matching row IDs (empty if not found)
   
Time: O(log_b(N)) = O(height) tree traversals + O(log b) binary searches per node
```

**Concurrency:** Read latch on each node; released as we descend (lock-free interior).

#### 3.2 Insert(key: &[u8], row_id: RowId) → Result<()>

```
1. Traverse to leaf (write latch on parent, read latch on child until leaf reached)
2. At leaf:
   - If unique and key exists: return DuplicateKey error
   - Insert key/row_id in sorted position
   - If leaf is full (key_count == branching_factor - 1):
       a) Split leaf: create sibling, move right half
       b) Promote middle key to parent
       c) Update sibling links: L.next -> R
       d) If parent is full: split parent (recursive)
       e) If root is full: create new root

Time: O(log_b(N)) + O(branching_factor) for split operations
```

**Split Decision:** Trigger only when key_count reaches branching_factor (not +1).

**Concurrency:** Write latch on each node; hold parent during child split to ensure consistency.

#### 3.3 Delete(key: &[u8], row_id: Option<RowId>) → Result<()>

```
1. Traverse to leaf (write latch chain, safe path descent)
2. At leaf:
   - Find key
   - If row_id specified: remove only that (key, row_id)
   - If row_id None: remove all rows for key
   - If leaf underflows (key_count < branching_factor / 2):
       a) Try borrow from left sibling
       b) If no spare: try borrow from right
       c) If no spare: merge with sibling
       d) Recursively fix parent underflow

Time: O(log_b(N)) + O(branching_factor) for merge operations
```

**Merge Decision:** Trigger when key_count < branching_factor / 2 (except root).

#### 3.4 RangeScan(start: &[u8], end: &[u8], inclusive_end: bool) → Cursor

```
1. Lookup(start_key) to find starting leaf
2. Position cursor at first key >= start_key
3. Return cursor; caller iterates with cursor.next():
     - Yield (key, row_id) while key <= end_key (or < if !inclusive_end)
     - At leaf end: follow next_leaf pointer
     - Stop at end_key boundary or None (last leaf)

Time: O(log_b(N)) startup + O(K) for K result rows
```

**Optimization:** Leaf linking enables sequential access without tree re-traversal.

### 4. Concurrency Model: Latch-Based with Deadlock Prevention

**Latching Strategy:**

| Operation | Latch Mode | Descent Pattern | Release Pattern |
|-----------|-----------|-----------------|-----------------|
| Lookup | READ | Acquire parent, acquire child, release parent (lock-free interior) | Release at leaf |
| Insert | WRITE | Acquire parent write; acquire child | If child not full: release parent, keep child. If child full: keep both |
| Delete | WRITE | Acquire parent write; acquire child | Keep chain until merge complete |
| Range Scan | READ | Acquire leaf read; follow next_leaf | Release prev leaf, acquire next |

**Deadlock Prevention (Standard B+ Tree Rule):**
```
Rule: Always acquire latches in top-down tree order.
  - No transaction acquires child latch before releasing parent
  - No cycles in lock dependency graph
  - Prevents classical B+ tree deadlock scenarios
```

**Implementation Pattern (Wave 18):**
```
// Safe path descent:
// 1. Acquire parent write latch
// 2. Examine child pointer
// 3. Acquire child write latch
// 4. If child is not full:
//      Release parent write latch (safe to proceed with child only)
// 5. Else:
//      Keep both; prepare to split
// 6. After split:
//      Release child, traverse to parent, promote key
```

**Lock Escalation (Deferred to Wave 20+):**
- For large range scans, consider tree-level read lock to reduce per-node overhead
- Not required for Wave 18 correctness

### 5. Recovery and Index Rebuild

**Failure Scenarios:**

| Scenario | Detection | Recovery |
|----------|-----------|----------|
| Crash during split | Incomplete parent key promotion | Detect via tree DFS; rebuild |
| Crash during merge | Orphaned node or dangling pointer | Detect via tree DFS; rebuild |
| Corrupted node | PageHeader validation fails | Detect at load; mark for rebuild |
| Complete index loss | Root page missing | Rebuild from table scan |

**Recovery Phases (Wave 18):**

```
Phase 1: Consistency Check (Startup)
  1. Load root page
  2. If root missing/corrupt: mark index NEEDS_REBUILD
  3. Else: DFS traverse tree, validate structure
  4. If any invalid structure: mark NEEDS_REBUILD
  5. Else: mark CONSISTENT

Phase 2: WAL Redo (If CONSISTENT)
  1. Replay index insert/delete/split records
  2. Update page_lsn on affected pages
  3. Flush indexes

Phase 3: Full Rebuild (If NEEDS_REBUILD)
  1. Truncate all index pages
  2. Scan heap table sequentially
  3. For each row: extract index key columns
  4. Call index.insert(key, row_id)
  5. Flush rebuilt index
```

**Rebuild Performance:**

| Operation | Complexity | Time Estimate (1M rows) |
|-----------|------------|----------------------|
| Heap scan | O(N) | ~5 sec (sequential disk) |
| Index rebuild (naive) | O(N log N) | ~60 sec (cascade splits) |
| Index rebuild (bulk load) | O(N) | ~10 sec (deferred Wave 19) |

**Wave 18 Constraint:** Naive rebuild acceptable; bulk load optimization deferred to Wave 19.

### 6. Index Metadata and Catalog Integration

**BTreeIndexMetadata (stored in catalog):**

```rust
struct BTreeIndexMetadata {
    index_id: u64,
    table_id: u64,
    columns: Vec<ColumnId>,    // Indexed columns
    is_unique: bool,           // Uniqueness constraint
    created_lsn: u64,          // Creation record LSN
    root_page_id: PageId,      // Current root (may change on rebuild)
    branching_factor: u16,     // Tunable; default 128
}
```

**Branching Factor Selection:**

| Page Size | Branching Factor | Node Size | Approx Height (1M keys) |
|-----------|-----------------|-----------|----------------------|
| 16 KiB | 128 | ~1.5 KiB | 4 |
| 32 KiB | 256 | ~3 KiB | 3 |

- Default: 128 for 16 KiB pages
- Tunable at index creation time
- Stored in metadata for rebuild consistency

### 7. Key Type Support

**Supported Index Key Types:**

- **Integers:** I8, I16, I32, I64, I128, U8, U16, U32, U64, U128
  - Serialized big-endian for byte-order-independent comparison
  
- **Text:** UTF-8, UTF-16, Unicode
  - Length-prefixed; collation-aware comparison
  - Prefix compression deferred to Wave 19
  
- **Decimal:** Fixed precision + scale
  - Serialized with sign + exponent
  - Byte-comparable after serialization
  
- **Bool:** Single byte (0x00 = false, 0x01 = true)

- **Timestamp:** 8-byte LSN or epoch
  - Serialized big-endian

**NULL Handling:**

```
NULL Representation:
  Prefix = 0xFF 0xFF (special marker)
  
NULL Ordering (SQL Standard):
  NULL < all non-NULL values
  Multiple NULLs are equal
  
Unique Index + NULL:
  Multiple NULLs permitted (per SQL standard)
  Duplicate non-NULL keys rejected
```

### 8. Testing and Validation Strategy (Wave 18+)

**Unit Tests (Wave 18):**
- Node split/merge invariant preservation
- Occupancy thresholds enforcement
- Binary search correctness
- Serialization/deserialization roundtrip

**Integration Tests (Wave 19):**
- Concurrent lookup + insert (non-conflicting keys)
- Range scans over various ranges
- Unique constraint enforcement
- Crash/recovery scenarios (simulated corruption)

**Performance Tests (Wave 20):**
- Lookup latency vs tree height
- Insert throughput vs branching factor
- Range scan throughput
- Recovery time vs table size

### 9. Constraints and Non-Goals

**Constraints:**
- ✅ Must reuse PageHeader/PageTrailer (DEC-032)
- ✅ Must integrate with BufferPool latch protocol
- ✅ Must support recovery via rebuild + redo
- ✅ Must enforce unique constraints correctly
- ✅ Must prevent deadlock via top-down latching
- ✅ Must support composite keys with NULL handling

**Non-Goals (Deferred):**
- ❌ Prefix compression (Wave 19+)
- ❌ Bulk-load optimization (Wave 19+)
- ❌ Latch-free data structure (post-Wave 20)
- ❌ Adaptive branching factor (Wave 20+)
- ❌ Index maintenance via incremental redo (beyond Wave 18)

### 10. Wave 18-20 Implementation Roadmap

**Wave 18: N2-BTREE-IDX-002, 003**
1. Design types + trait definitions (DONE: this DEC)
2. Node management (load, create, serialize, deserialize)
3. Insert/delete with split/merge (latched descent)
4. Basic range scan cursor

**Wave 19: N2-BTREE-IDX-004, 005**
1. Range scan optimization (leaf link traversal)
2. Bulk-load index construction
3. Crash recovery + rebuild
4. Integration tests

**Wave 20: N2-BTREE-IDX-006**
1. Performance tuning (adaptive branching factor)
2. Latch-free experiments (optional)
3. Stress tests + production readiness

---

## Rejected Alternatives

### 1. B-Tree (not B+ Tree)

**Rejected because:** Data in internal nodes complicates recovery and splits.

**B+ Tree chosen because:** All data in leaves; internal nodes are guides only; enables:
- Clean recovery (rebuild from heap + redo)
- Efficient range scans (leaf sibling links)
- Simpler merge logic (no data movement from parent)

### 2. Latch-Free Data Structure

**Rejected because:** Complexity beyond Wave 18 scope; high risk of subtle concurrency bugs.

**Latch-based chosen because:**
- Industry-standard B+ Tree concurrency
- Deadlock prevention via top-down latching is well-understood
- Acceptable performance for OLTP workloads
- Future migration to latch-free possible post-Wave 20

### 3. Immediate Index Rebuild Post-Crash

**Rejected because:** User-visible delay unacceptable; recovery must support deferred rebuild.

**Deferred rebuild chosen because:**
- Phase 1: Consistency check (fast)
- Phase 2: WAL redo (if consistent)
- Phase 3: Rebuild only if inconsistent (background or on-demand)

### 4. Custom Page Header for Index Pages

**Rejected because:** DEC-032 forbids custom headers; violates storage contracts.

**Standard PageHeader chosen because:**
- Ensures HotStore/ColdStore interoperability
- Simplifies recovery (no special index-page handling)
- Aligns with storage engine architecture

---

## Risks and Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|-----------|
| Split cascade on full tree | Slow inserts | Low | Reduce branching_factor; trigger split earlier |
| Deadlock in latching protocol | System hang | Medium | Strict top-down latching rule + deadlock detector (Wave 19) |
| Index corruption post-crash | Data unavailability | Medium | Robust consistency check; aggressive rebuild triggering |
| Slow recovery (naive rebuild) | Long startup time | High | Bulk-load optimization (Wave 19); async rebuild (Wave 20) |
| Memory explosion (large keys) | OOM on internal nodes | Low | max_key_size constraint; enforce at insert time |
| Concurrency bottleneck (root contention) | Throughput ceiling | Medium | Root node caching (Wave 20+); better split strategy |

---

## Validation Criteria

**Design Acceptance (This DEC):**
- ✅ All invariants specified and derivable from operations
- ✅ Algorithms (lookup, insert, delete, range scan) described with complexity analysis
- ✅ Concurrency model defined with deadlock prevention rules
- ✅ Recovery strategy integrates with Wave 17 WAL design
- ✅ Type definitions compile (no implementations, all `todo!()`)
- ✅ Integration points identified (BufferPool, Catalog, Recovery)

**Implementation Acceptance (Wave 18):**
- ✅ All trait methods implemented (no more `todo!()`)
- ✅ Unit tests cover split/merge/balance invariants
- ✅ Concurrent operations (lookup + insert) pass correctness tests
- ✅ Range scans over all key types complete without corruption
- ✅ Unique constraint enforcement blocks duplicates
- ✅ Recovery rebuild restores correct tree structure

---

## References

- **DEC-032:** Storage Durable Page Format Baseline
- **Wave 17:** BufferPool and Heap Page Engine design
- **Wave 17:** WAL Recovery and durability ordering
- **CLRS (2009):** Introduction to Algorithms (B-tree chapter)
- **Ramakrishnan & Gehrke (2002):** Database Management Systems

---

## Sign-Off

| Role | Name | Date | Status |
|------|------|------|--------|
| Storage Architect | (design author) | Q2 2026 | ACCEPTED |
| Recovery Specialist | (handoff reviewer) | (pending) | PENDING |
| Doctrine Guardian | (enterprise constraints) | (pending) | PENDING |

---

## Appendix: B+ Tree Invariant Checklist (For Wave 18 Implementation)

**After every operation, verify:**

- [ ] `Inv-1: Leaf Balance` — All leaves at same depth
- [ ] `Inv-2: Key Ordering` — Keys sorted within and across nodes
- [ ] `Inv-3: Child Pointers` — Child keys match key boundaries
- [ ] `Inv-4: Occupancy` — key_count in valid range
- [ ] `Inv-5: Leaf Linking` — next_leaf pointers correct and consistent
- [ ] `Inv-6: Key Completeness` — All keys reachable from root, all in leaf
- [ ] `Inv-7: Unique Constraint` — Duplicates rejected (if unique index)

**Recovery Validation (Phase 1):**

- [ ] Root page exists and loads without corruption
- [ ] Root.is_leaf = false OR (tree height = 1)
- [ ] DFS traverse all nodes; verify Inv-1 through Inv-6
- [ ] If any invariant violated: mark NEEDS_REBUILD

**Reconstruction Validation (Phase 3):**

- [ ] Heap scan produces (key, row_id) pairs in order
- [ ] Insert each pair via index.insert()
- [ ] Final tree satisfies all invariants
- [ ] Row count matches table row count

