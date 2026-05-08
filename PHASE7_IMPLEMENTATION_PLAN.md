# Phase 7 Implementation Plan: Storage Recovery - Durability Gates & Crash Matrix

## Current State (Verified)

✅ **Boundary crates exist with tests passing:**
- andromeda-storage-page (9 tests)
- andromeda-manifest (2 tests)
- andromeda-segment (3 tests)
- andromeda-buffer-pool (2 tests)
- andromeda-recovery (1 test)
- andromeda-restore (4 tests)
- andromeda-hadr (47 tests)

✅ **andromeda-storage:** 360 tests passing (facade still exists)

## Phase 7 Concrete Work Items

### 1. Durability Gate Implementation (HIGH PRIORITY)

**1a. Page Ownership Invariants**
- ✅ Exists: andromeda-storage-page::PageFlushDurabilityBoundary
- ✅ Test: tests::page_store_enforces_wal_before_page_flush_precondition
- Task: Create canonical source test verifying PageId + LSN owned by page + wal crates

**1b. Manifest Atomic Switch**
- ✅ Exists: andromeda-manifest::validate_manifest_atomic_switch
- ✅ Test: tests::manifest_switch_and_recovery_fences_validate
- Task: Add comprehensive atomic switch proof test

**1c. Recovery Floor Valid**
- ✅ Exists: andromeda-manifest::ManifestDurabilityBoundary::can_start_recovery_at
- ✅ Test: tests::manifest_boundary_validates_identity_crc_and_recovery_floor
- Task: Test recovery never starts before durable manifest + wal floor

**1d. Backup Immutability**
- Status: andromeda-backup crate exists but needs immutability enforcement test
- Task: Implement BackupArtifact::mark_immutable() and test write rejection

**1e. Restore Consistency**
- ✅ Exists: andromeda-restore tests for PITR consistency
- Task: Add comprehensive restored-state consistency verification

**1f. HADR Quorum Logic**
- ✅ Exists: 47 tests in andromeda-hadr
- Tasks: 
  - Add split-brain prevention test
  - Add single-primary only-writes test
  - Add replica promotion eligibility test

**1g. Crash-Recovery Matrix**
- Status: Does not exist yet
- Task: Create crash injection test matrix validating recovery at N crash points

### 2. Code Extraction from andromeda-storage

**Current andromeda-storage modules that need extraction:**
- backup → andromeda-backup (move implementation)
- recovery → andromeda-recovery (move implementation)
- restore_orchestration → andromeda-restore (move implementation)
- segment_index → andromeda-segment (move implementation)
- buffer_pool → andromeda-buffer-pool (move implementation)
- hadr → andromeda-hadr (move implementation)

### 3. Create Missing Crate: andromeda-page-store

**Status:** Does not exist as dedicated crate yet
**Purpose:** Disk page read/write, page cache, LRU eviction
**Modules to extract from andromeda-storage:**
- disk_manager (partial - page I/O methods)

**Implementation:** PageStore trait with InMemoryPageStore and DiskPageStore impls

### 4. Thin andromeda-storage Facade

**Remaining responsibilities:**
- Orchestration: coordinate page-store, buffer-pool, wal, manifest, recovery
- Compatibility layer: reexport key types for call sites
- Durability_fence.rs: already validates page + manifest + wal boundaries

**Reduction target:** 80% of modules extracted to dedicated crates

### 5. Dependency Graph Validation

**Required DAG (no cycles):**
```
andromeda-storage-page ← andromeda-page-store ← andromeda-buffer-pool
andromeda-wal ← andromeda-storage ← andromeda-recovery
andromeda-manifest ← andromeda-storage
andromeda-storage-page ← andromeda-manifest
andromeda-backup ← andromeda-restore
```

### 6. Test Implementation Order

1. **Phase 1:** Verify all 8 boundary crates compile + tests pass
2. **Phase 2:** Add durability gate tests (page flush, manifest switch, recovery floor)
3. **Phase 3:** Add backup immutability + restore consistency tests
4. **Phase 4:** Add HADR quorum + split-brain tests
5. **Phase 5:** Add crash-recovery matrix (N crash points tested)
6. **Phase 6:** Extract code from andromeda-storage
7. **Phase 7:** Verify all 360+ tests pass with new structure

## Success Criteria

✅ All 8 boundary crates have tests passing
✅ Durability gate tests validate page flush, manifest switch, recovery floor
✅ Backup immutability enforced (write to immutable artifact → error)
✅ Restore consistency verified (restored DB passes checks)
✅ HADR split-brain prevention working
✅ Crash-recovery matrix: all crash points tested
✅ Dependency graph is DAG (no cycles)
✅ andromeda-storage reduced to thin facade
✅ cargo test --workspace passes
✅ No compiler warnings from Phase 7 work

## Implementation Timeline

- **Step 1-3:** Add durability gate tests (30 min)
- **Step 4:** Create andromeda-page-store (15 min)
- **Step 5:** Add crash-recovery matrix (45 min)
- **Step 6:** Extract code from andromeda-storage (60 min)
- **Step 7:** Verify all tests pass (30 min)
- **Total:** ~2.5 hours

## Notes

- Phase 7 assumes Phase 2 (WAL) + Phase 6 (Transaction) completed
- All crates must forbid unsafe_code (already declared in lib.rs)
- GPU must never participate in recovery or WAL (already enforced)
- Backup is advisory retention (not truth source)
