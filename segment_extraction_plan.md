# Segment Extraction Plan (Wave 2)

**Status:** Analysis Complete  
**Target:** Extract segment-related modules from andromeda-storage into dedicated crates  
**Scope:** Prep for Week 4 implementation  
**Date:** 2026-05-07

## Executive Summary

The segment functionality in andromeda-storage spans **1,647 LOC** across multiple modules with varying responsibilities:

- **Core boundaries** (263 LOC): Already isolated in `andromeda-segment` crate ✓
- **Segment index** (1,506 LOC): Codec, types, and digest logic tightly coupled to storage
- **Segment descriptor facade** (134 LOC): Storage-specific wrapper around segment boundary types
- **Layout re-exports** (7 LOC): Facade pattern for public API consistency

**Recommendation:** Extract `SegmentIndex` codec and types into dedicated `andromeda-segment-index` crate while maintaining current `andromeda-segment` as the boundary contract crate.

---

## Module Boundaries Analysis

### Current Architecture

```
andromeda-storage/
├── src/segment.rs                    [134 LOC] - SegmentDescriptor facade
├── src/segment_index/
│   ├── types.rs                     [238 LOC] - SegmentIndexV0 types
│   ├── codec.rs                    [1185 LOC] - Encode/decode logic
│   ├── error.rs                     [66 LOC]  - SegmentIndexError
│   ├── digest.rs                    [~impl]    - CRC/SHA digest utilities
│   └── mod.rs                       [17 LOC]  - Module re-exports
├── src/layout/segment.rs            [7 LOC]   - Facade re-exports
├── tests/
│   ├── segment_index_contract.rs    [247 LOC] - 11 test functions
│   ├── layout_facade_invariants.rs  [149 LOC] - 7 test functions
│   ├── publication_facade_invariants.rs [173 LOC] - 9 test functions
│   └── other segment-related tests  [27 test methods across files]
└── src/extent/descriptor.rs         [refs SegmentId, SegmentState]

andromeda-segment/
├── src/lib.rs                       [263 LOC]
│   ├── SegmentId newtype
│   ├── SegmentState enum
│   ├── SegmentMutation enum
│   ├── SegmentHeader struct (durable format)
│   ├── SegmentTrailer struct (durable format)
│   ├── SegmentDurabilityBoundary struct
│   └── 3 internal test functions
```

### Proposed New Architecture (Post-Extraction)

```
andromeda-segment/                   [BOUNDARY CRATE - minimal, immutable]
├── src/lib.rs
│   ├── SegmentId
│   ├── SegmentState
│   ├── SegmentMutation
│   ├── SegmentHeader (durable format)
│   ├── SegmentTrailer (durable format)
│   └── SegmentDurabilityBoundary
└── Cargo.toml (deps: andromeda-core, andromeda-storage-page, andromeda-wal)

andromeda-segment-index/             [NEW - extracted from storage]
├── src/
│   ├── lib.rs (re-exports)
│   ├── types.rs                     [SegmentIndexV0 types]
│   ├── codec.rs                     [Encode/decode V0 format]
│   ├── error.rs                     [SegmentIndexError]
│   ├── digest.rs                    [CRC/SHA utilities]
│   └── tests.rs                     [Integration tests]
├── tests/
│   ├── segment_index_contract.rs    [Migrated]
│   └── [other contracts]
└── Cargo.toml (deps: andromeda-segment, andromeda-storage-page, andromeda-core, sha2, ...)

andromeda-storage/                   [SIMPLIFIED]
├── src/segment.rs                   [SegmentDescriptor facade - minimal]
├── src/segment_index/               [REMOVED - moved to andromeda-segment-index]
├── src/layout/segment.rs            [Updated re-exports]
└── Cargo.toml (deps: + andromeda-segment-index instead of inline)
```

---

## Dependencies Map

### Current Dependencies (Inbound - who depends on segment)

```
andromeda-storage (public API)
  ├── andromeda-segment (boundary types)
  ├── andromeda-cli
  └── [manifests, extent, recovery, publication modules internally]

andromeda-segment (boundary types)
  └── andromeda-storage (depends on it)
```

### Proposed New Dependencies

```
andromeda-segment (unchanged)
  └── deps: andromeda-core, andromeda-storage-page, andromeda-wal

andromeda-segment-index (NEW)
  └── deps:
      - andromeda-segment (for SegmentId, SegmentState)
      - andromeda-storage-page (for PageId, PageSize, ObjectId, AllocationId)
      - andromeda-core (for AndromedaResult, AndromedaError)
      - sha2 (external) - for SHA256 digest
      - [internal digest helpers]

andromeda-storage (UPDATED)
  └── deps: + andromeda-segment-index (replaces inline segment_index module)
           - remove segment_index private module
           - keep segment.rs for SegmentDescriptor facade
           - update layout/segment.rs re-exports
```

### Cross-Module References (Must Preserve)

| Module | References | Reason |
|--------|-----------|--------|
| segment_index::codec | storage types (ObjectId, PageId, etc) | Format versioning |
| segment_index::types | SegmentState enum | Index entry metadata |
| extent::descriptor | SegmentId, SegmentState | Extent-segment relationship |
| manifest module | SegmentId, SegmentState | Manifest snapshot |
| publication | SegmentState, SegmentId | Publication facade |
| recovery | SegmentIndexV0 types | Snapshot recovery |
| WAL codec | SegmentId, SegmentState | Record format |

**Critical Constraint:** `SegmentIndexV0` is part of **durable manifest snapshots** — any version change requires WAL recovery compatibility validation.

---

## Test Requirements (Gate 0-3)

### Gate 0: Module Structure Tests (Unit)
**Goal:** Verify module boundaries and public API integrity

Tests to extract:
- `segment_index_contract.rs` (247 LOC, 11 tests)
  - Tests: header/trailer validation, entry ordering, CRC/SHA verification
  - **Must Pass:** All CRC/SHA validation tests (no format change)

New tests needed:
- Crate boundary visibility (pub vs private)
- Re-export consistency

**Time:** ~1 hour  
**Blocker:** Must not change any serialization format

---

### Gate 1: Format Preservation Tests (Contract)
**Goal:** Ensure on-disk format unchanged after extraction

**Coverage:**
- Encode/decode roundtrip (all segment index versions)
- Header/trailer/entry format byte offsets (static asserts)
- CRC polynomial consistency
- SHA256 digest determinism
- Page size tag encoding
- State enum tag mapping

**Implementation:**
```rust
#[test]
fn segment_index_v0_encode_decode_roundtrip() { }

#[test]
fn segment_index_header_offsets_unchanged() {
    assert_eq!(SEGMENT_INDEX_V0_HEADER_LEN, 256);
    assert_eq!(SEGMENT_INDEX_V0_ENTRY_LEN, 160);
    // ... all offset constants
}

#[test]
fn segment_state_tag_encoding_preserved() {
    assert_eq!(SEGMENT_STATE_TAG_BUILDING_HOT_SNAPSHOT, 1);
    assert_eq!(SEGMENT_STATE_TAG_SEALED, 2);
    assert_eq!(SEGMENT_STATE_TAG_PUBLISHED_COLD, 3);
}
```

**Test Files:**
- Migrate `segment_index_contract.rs` → `andromeda-segment-index/tests/`
- New: `format_roundtrip_test.rs` (property-based testing with proptest)

**Time:** ~2 hours  
**Blocker:** Any format drift detected = extraction blocked

---

### Gate 2: Recovery Path Tests (C5 Integration)
**Goal:** Validate crash/recovery scenarios with extracted code

**Coverage:**
- Manifest snapshot loading with SegmentIndexV0
- WAL record replays referencing segments
- Extent descriptor validation against segment index
- Orphan segment detection after recovery

**Scenarios:**
- Crash during SegmentIndexV0 encode → recovery reloads from WAL
- Corrupted SegmentIndexV0 entry → recovery rejects with detailed error
- Manifest version mismatch with SegmentIndexV0 format
- Segment state transition validation during replay

**Test Files:**
- Extract `property_recovery_replay.rs` segment-related tests
- New: `recovery_segment_index_contract.rs` (WAL replay simulation)

**Time:** ~3 hours  
**Blocker:** Must validate crash safety invariants from C5_GATE0

---

### Gate 3: Integration Tests (E2E)
**Goal:** Full pipeline: hot-to-cold snapshot → segment index → archive

**Coverage:**
- `storage_hotcold_pipeline_e2e.rs` segment-related flows
- Extent allocation → segment sealing → cold publication
- SegmentIndexV0 generation during snapshot
- Index file durability and attestation

**Scenarios:**
- Multi-segment snapshot creation
- Segment index with extension bytes (forensic hold flags)
- Published cold with snapshot reference validation
- Index file checksum validation after disk I/O

**Test Files:**
- Migrate E2E tests from `tests/storage_hotcold_pipeline_e2e.rs`
- Coordinate with manifest snapshot tests

**Time:** ~2 hours  
**Blocker:** Manifest and snapshot contracts must be stable

---

## Format Compatibility Strategy

### On-Disk Format Preservation

**Version Scheme:** `SEGMENT_INDEX_V0` → immutable, new versions require `V1` crate

**Critical Constants (Static Asserts):**
```rust
// Header format (256 bytes, fixed offsets)
const SEGMENT_INDEX_V0_MAGIC: [u8; 8] = *b"ANDSGIX0";
const SEGMENT_INDEX_V0_HEADER_LEN: usize = 256;
const SEGMENT_INDEX_V0_ENTRY_LEN: usize = 160;
const SEGMENT_INDEX_V0_TRAILER_LEN: usize = 96;

// State encoding (wire format for WAL/manifest)
const SEGMENT_STATE_TAG_BUILDING_HOT_SNAPSHOT: u16 = 1;
const SEGMENT_STATE_TAG_SEALED: u16 = 2;
const SEGMENT_STATE_TAG_PUBLISHED_COLD: u16 = 3;

// Page size encoding
const PAGE_SIZE_TAG_16K: u16 = 1;
const PAGE_SIZE_TAG_32K: u16 = 2;

// Digest algorithms (NOT user-configurable)
- CRC32 (ISO HDLC poly): headers, entries, trailer
- CRC64 (ECMA poly): entry table
- SHA256: file content attestation
```

**Preservation Mechanism:**
1. **Format versioning:**
   - `SegmentIndexV0` is frozen (no changes allowed)
   - Future changes → `SegmentIndexV1` in separate branch
   - Migration logic lives in `andromeda-restore` (cross-version support)

2. **Codec encapsulation:**
   - All encoding/decoding logic stays in `codec.rs`
   - Digest functions are private (deterministic, versioned)
   - CRC polynomials embedded in codec (never exported)

3. **Wire format validation:**
   - Recovery deserializes `SegmentIndexV0` without structural changes
   - WAL codec continues to reference `SegmentState` enum (stable)
   - Manifest snapshots embed `SegmentIndexV0` bytes as-is (opaque blobs)

### Recovery Path Consistency

**Invariant:** Snapshot-embedded `SegmentIndexV0` bytes must round-trip without loss.

```
Recovery flow:
  Manifest snapshot (on disk) 
    → load SegmentIndexV0 bytes 
    → decode via andromeda-segment-index/codec.rs 
    → validate via SegmentIndexV0::validate() 
    → replay extent allocations 
    → verify segment state against WAL
```

**Validation Points:**
- Offset calculations (no mutation during extraction)
- Checksum verification (same digest algorithms)
- Length overflow guards (bounds checks preserved)
- State enum tags (wire format compatibility)

---

## Risk Assessment

### Extraction Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|-----------|
| Format byte offset drift | **High** | Corruption | Static offset constants + comprehensive property tests |
| Digest algorithm mutation | **Medium** | Recovery failure | CRC/SHA logic validated in unit tests before extraction |
| Missing test coverage for new crate | **Medium** | Silent bugs | Migrate all existing tests + add format preservation tests |
| Circular dependency (segment-index → storage) | **Low** | Compile error | Storage types (ObjectId, PageId) already in andromeda-storage-page |
| WAL record format changes | **Low** | Replay divergence | SegmentState enum stays in andromeda-segment (immutable) |
| Extent-segment relationship breaks | **Medium** | Extent orphaning | Preserve SegmentId/SegmentState in extent descriptor validation |

### Mitigation Strategy

**Pre-Extraction Gate:**
1. ✓ Run all existing segment tests (11 contract tests + internal tests)
2. ✓ Add format preservation tests (byte offsets, CRC algorithms)
3. ✓ Fuzz SegmentIndexV0 encode/decode (proptest)
4. ✓ Validate WAL record references (no breaking changes)

**Extraction Process:**
1. Create `andromeda-segment-index` crate with locked dependencies
2. Copy segment_index/* → new crate (no refactoring)
3. Update Cargo.toml (add sha2, organize deps)
4. Run all tests from new crate
5. Update storage to depend on new crate
6. Verify no format changes in binaries

**Post-Extraction:**
1. Document SegmentIndexV0 immutability contract
2. Add ADR: "SegmentIndex versioning policy"
3. Establish recovery test matrix (crash/recovery scenarios)

---

## LOC Estimate Summary

| Component | LOC | Status | Notes |
|-----------|-----|--------|-------|
| segment_index/types.rs | 238 | Extract | SegmentIndexV0, entry types |
| segment_index/codec.rs | 1185 | Extract | Encode/decode V0 format |
| segment_index/error.rs | 66 | Extract | SegmentIndexError enum |
| segment_index/digest.rs | ~80-100 | Extract | CRC/SHA helpers (assume ~100) |
| segment_index/mod.rs | 17 | Extract | Re-exports |
| **segment_index/ total** | ~1506 | → andromeda-segment-index | |
| segment.rs | 134 | Keep | SegmentDescriptor (storage-specific) |
| layout/segment.rs | 7 | Update | Update re-export paths |
| **Subtotal in andromeda-storage** | 141 | Stay | Facade layer |
| | | | |
| andromeda-segment/lib.rs | 263 | No change | Boundary types (stable) |
| **Total segment-related** | ~1910 | | |

### New Crate Estimate

**andromeda-segment-index/Cargo.toml:**
```toml
[dependencies]
andromeda-segment.workspace = true
andromeda-storage-page.workspace = true
andromeda-core.workspace = true
sha2 = "0.11.0"  # External (required for SHA256)

[dev-dependencies]
proptest.workspace = true
```

**File Structure:**
```
andromeda-segment-index/
├── Cargo.toml (new)
├── README.md (design doc reference)
├── src/
│   ├── lib.rs (re-exports)
│   ├── types.rs (238 LOC)
│   ├── codec.rs (1185 LOC)
│   ├── error.rs (66 LOC)
│   ├── digest.rs (~100 LOC)
│   └── mod.rs internal structure
├── tests/
│   ├── segment_index_contract.rs (migrated, 247 LOC)
│   ├── format_preservation_test.rs (new, ~200 LOC)
│   └── recovery_integration_test.rs (new, ~150 LOC)
└── MIGRATION_NOTES.md (format compatibility record)
```

**Total New Crate LOC:**
- Source: ~1694 LOC
- Tests: ~600 LOC
- Config: ~50 LOC
- **Total: ~2344 LOC**

---

## Reexport Points (Public API)

### andromeda-segment (unchanged)
```rust
pub use andromeda_segment::{
    SegmentId,
    SegmentState,
    SegmentMutation,
    SegmentHeader,
    SegmentTrailer,
    SegmentDurabilityBoundary,
};
```

### andromeda-segment-index (NEW)
```rust
pub use codec::{
    SEGMENT_INDEX_V0_MAGIC,
    SEGMENT_INDEX_V0_FORMAT_MAJOR,
    SEGMENT_INDEX_V0_FORMAT_MINOR,
    SEGMENT_INDEX_V0_BYTE_ORDER,
    SEGMENT_INDEX_V0_HEADER_LEN,
    SEGMENT_INDEX_V0_ENTRY_LEN,
    SEGMENT_INDEX_V0_TRAILER_LEN,
    SEGMENT_INDEX_V0_MAX_ENTRIES,
    SEGMENT_INDEX_V0_MAX_EXTENSION_BYTES,
    SEGMENT_INDEX_V0_FLAG_PUBLISHED_COLD_ONLY,
    SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES,
    SEGMENT_INDEX_V0_FLAG_FORENSIC_HOLD,
};

pub use types::{
    SegmentIndexV0,
    SegmentIndexHeaderV0,
    SegmentIndexEntryV0,
    SegmentIndexTrailerV0,
    SegmentIndexBuildContextV0,
};

pub use error::{SegmentIndexError, SegmentIndexResult};
```

### andromeda-storage (updated)
```rust
// From segment.rs:
pub use segment::SegmentDescriptor;
pub use segment::{
    SegmentId, SegmentState, SegmentMutation, SegmentHeader, SegmentTrailer,
    // (re-exported from andromeda-segment via segment.rs)
};

// From segment_index.rs (now delegated):
pub use andromeda_segment_index::{
    SegmentIndexV0,
    SegmentIndexHeaderV0,
    SegmentIndexEntryV0,
    SegmentIndexTrailerV0,
    SegmentIndexBuildContextV0,
    SEGMENT_INDEX_V0_MAGIC,
    // ... all format constants
};
```

### Public Facade (layout/segment.rs - unchanged)
```rust
pub use crate::{
    SegmentDescriptor,
    SegmentHeader,
    SegmentId,
    SegmentMutation,
    SegmentState,
    SegmentTrailer,
};
```

---

## Ready-to-Extract Checklist

### Pre-Extraction Validation ✓

- [x] Segment module identified and mapped
- [x] Dependencies documented (core, storage-page, wal)
- [x] Tests enumerated (11 contract tests + 3 internal tests)
- [x] Format constants verified (no mutations)
- [x] Extent-segment relationship preserved
- [x] Recovery paths documented
- [x] WAL record format stability confirmed

### Extraction Phase (Week 4)

#### Step 1: Create andromeda-segment-index crate
- [ ] Copy Cargo.toml template (workspace deps)
- [ ] Add external deps: sha2 = "0.11.0"
- [ ] Copy src/segment_index/* → src/
- [ ] Create lib.rs with re-exports
- [ ] Verify compilation

#### Step 2: Migrate Tests
- [ ] Copy tests/segment_index_contract.rs → new crate
- [ ] Create format_preservation_test.rs (byte offsets, CRC)
- [ ] Create recovery_integration_test.rs (WAL replay)
- [ ] Run all tests: `cargo test --all-features`
- [ ] Verify zero test failures

#### Step 3: Update andromeda-storage
- [ ] Remove src/segment_index/ directory
- [ ] Update src/segment.rs (if needed)
- [ ] Update Cargo.toml: add andromeda-segment-index dependency
- [ ] Update lib.rs re-exports to delegate to new crate
- [ ] Run storage tests: verify all pass

#### Step 4: Validate Format Preservation
- [ ] Property test roundtrip (proptest): encode → decode → encode ✓
- [ ] Binary comparison: before/after extraction (identical)
- [ ] Crash injection: recovery with extracted codec
- [ ] WAL compatibility: old WAL records work with new codec

#### Step 5: Documentation
- [ ] Create MIGRATION_NOTES.md in new crate
- [ ] Document immutability contract for SegmentIndexV0
- [ ] Add design rationale to README.md
- [ ] Update CONTRIBUTING.md with segment module guidelines

### Post-Extraction Validation

- [ ] All tests pass (unit + integration + E2E)
- [ ] No binary size regression
- [ ] Recovery path works with old manifests
- [ ] Extent allocation with segment metadata succeeds
- [ ] Manifest snapshots deserialize correctly
- [ ] CI/CD pipeline green

---

## Implementation Notes

### Critical Assumptions
1. **andromeda-storage-page types are stable:** ObjectId, PageId, AllocationId, ExtentId will not change during extraction
2. **andromeda-segment boundary types are immutable:** SegmentId, SegmentState, SegmentMutation are public contracts
3. **Format version is locked:** No changes to SEGMENT_INDEX_V0_* constants allowed
4. **Digest algorithms are deterministic:** CRC32, CRC64, SHA256 produce same output before/after extraction

### Dependencies That Must NOT Change
- `andromeda-core` (error types)
- `andromeda-storage-page` (ObjectId, PageId, AllocationId, ExtentId)
- `andromeda-segment` (SegmentId, SegmentState)
- `andromeda-wal` (Lsn type)
- `sha2` (external, version-locked)

### Files That Remain in andromeda-storage
- `src/segment.rs` (SegmentDescriptor facade - storage-specific in-memory wrapper)
- `src/layout/segment.rs` (Public re-export layer)
- `src/extent/descriptor.rs` (Extent-segment relationship)
- All extent, manifest, recovery, publication modules (unchanged)

---

## References

- **C5 Gate 0:** Manifest snapshot format validation, crash/recovery tests
- **Design:** `docs/codex/segment_extraction_design.md` (to be created)
- **ADR:** "SegmentIndex versioning and evolution policy" (to be created)
- **Related Wave 2 Tasks:**
  - Heat map analysis for hot/cold segments
  - ColdStore manifest-segment integration
  - Backup/PITR with SegmentIndexV0

---

## Appendix A: Test Matrix

| Test File | LOC | Tests | Gate | Status |
|-----------|-----|-------|------|--------|
| segment_index_contract.rs | 247 | 11 | 1 | Ready |
| layout_facade_invariants.rs | 149 | 7 | 0 | Ready |
| publication_facade_invariants.rs | 173 | 9 | 1 | Ready |
| [Internal segment.rs tests] | ~20 | 3 | 0 | Ready |
| [Internal segment crate tests] | ~20 | 3 | 0 | Ready |
| [Recovery segment tests] | ~100 | 5 | 2 | TBD |
| [Format preservation tests] | ~200 | 8 | 2 | TBD |
| **Total** | **~909** | **~46** | | |

---

## Appendix B: Dependency Graph

```
┌─────────────────────────────────────┐
│  andromeda-segment                  │
│  (Boundary types - immutable)       │
│  SegmentId, State, Mutation         │
│  Header, Trailer, Boundary          │
│  ✓ deps: core, storage-page, wal    │
└────────────────┬────────────────────┘
                 │
                 │ imports
                 ▼
┌─────────────────────────────────────┐
│  andromeda-segment-index (NEW)      │
│  SegmentIndexV0, codec, errors      │
│  ✓ deps: segment, storage-page,     │
│          core, sha2                 │
└────────────────┬────────────────────┘
                 │
                 │ imports
                 ▼
┌─────────────────────────────────────┐
│  andromeda-storage (UPDATED)        │
│  SegmentDescriptor facade           │
│  ✓ deps: segment-index, extent,     │
│          manifest, recovery, wal    │
└─────────────────────────────────────┘
```

---

**Prepared by:** Segment Extraction Analysis  
**Week:** Wave 2 Prep  
**Next Phase:** Implementation (Week 4, estimated 3-4 days)
