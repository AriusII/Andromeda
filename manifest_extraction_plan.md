# Wave 2: Manifest Extraction Plan (Week 4 Preparation)

**Status:** Analysis Complete  
**Date:** 2026-05-07  
**Target:** Extract manifest module from `andromeda-storage` into dedicated `andromeda-manifest` crate  
**Risk Class:** C4-C5 (Mission-critical storage boundary)

---

## Executive Summary

The manifest module in `andromeda-storage` (552 LOC) manages database durability boundaries, snapshot references, and storage format contracts—all mission-critical C5 invariants. The `andromeda-manifest` crate already exists as a boundary contract stub (132 LOC). This extraction formalizes the module boundary, moves implementation code to the standalone crate, and establishes clear reexport points in storage.

**Deliverables:**
- Extract ~420 LOC of implementation (manifest.rs + submodules) into `andromeda-manifest`
- Retain ~75 LOC in `andromeda-storage` as reexport facade
- Validate crash/recovery behavior (C5 gate requirement)
- Establish durable WAL coverage proofs
- Ready crate for Week 4 standalone deployment

---

## Current State Analysis

### Module Inventory

**Location:** `crates/andromeda-storage/src/manifest/`

| File | LOC | Purpose | Boundary |
|------|-----|---------|----------|
| `manifest.rs` (entry) | 75 | Public API, DatabaseManifest struct, reexports | C5 |
| `cold_publication.rs` | 93 | Cold segment publication plans, placement boundary | C4 |
| `format.rs` | 92 | Storage format fingerprints, versioning | C5 |
| `hash.rs` | 54 | XorShift-based manifest hash (internal) | C5 |
| `snapshot.rs` | 91 | Snapshot publication, segment references | C5 |
| `tests.rs` | 147 | Unit tests (crash/recovery validation) | C5 |
| **Total** | **552** | | |

### Existing Manifest Crate

**Location:** `crates/andromeda-manifest/src/lib.rs` (132 LOC)

**Current content:**
- `ManifestDurabilityBoundary` struct (WAL/checkpoint proof)
- `validate_manifest_atomic_switch()` (C5 fence check)
- Module docstring (C5 invariants)
- Dependencies: `andromeda-core`, `andromeda-wal`

**Status:** Boundary contract stub; ready for implementation imports.

### Dependencies Graph

```
andromeda-storage
  ├─> depends on: andromeda-manifest (already)
  ├─> exports: manifest::* (re-exports + DatabaseManifest)
  └─> submodules: cold_publication, format, hash, snapshot

andromeda-recovery
  └─> depends on: andromeda-manifest
      └─> uses: ManifestDurabilityBoundary for crash-recovery matrix

andromeda-backup
  └─> does NOT depend on manifest (mission-critical boundary)

andromeda-segment
  └─> does NOT import manifest (immutable segment metadata)

andromeda-catalog
  └─> does NOT import manifest (catalog versioning separate)
```

### Reverse Dependencies

**Crates that import from storage::manifest:**
- 24 storage integration tests reference manifest types
- Recovery startup uses ManifestDurabilityBoundary
- Restoration boundary checks (recovery module)

**Test files that reference manifest:**
- `api_compat_reexports.rs` (API surface validation)
- `crash_recovery_impl.rs` (C5 recovery path)
- `backup_execution_plan.rs` (backup boundary)
- `backup_physical_plan_contract.rs` (backup plan)
- `recovery_contract.rs` (recovery floor)
- `recovery_completeness_contract.rs` (redo plan)
- `layout_publication_contract.rs` (snapshot publication)
- `storage_hotcold_pipeline_e2e.rs` (cold publication)
- `publication_facade_invariants.rs` (snapshot facade)
- 15 other tests (manifest validation in various contexts)

---

## Module Boundary Design

### What Moves to `andromeda-manifest` (420 LOC)

**Primary implementation:**
1. `DatabaseManifest` struct (complete definition + all methods)
2. `StorageFormatManifest` (format versioning contracts)
3. `ColdSegmentPublicationPlan` (placement decisions)
4. `DatabaseSnapshotPublication`, `SnapshotSegmentReference`
5. `SnapshotAvailabilityContract` (snapshot lifecycle)
6. All hashing logic (`storage_format_manifest_hash`, mix lanes)
7. Validation functions (manifest, snapshot, format, cold publication)
8. Storage error wrapper function
9. All unit tests (tests.rs)

**New dependencies for andromeda-manifest:**
- `andromeda-core` (error, result, catalog version)
- `andromeda-wal` (Lsn, LSN arithmetic)
- `andromeda-observe` (ManifestEventKind, TraceId, ManifestTrace) **[NEW]**
- `andromeda-segment` (SegmentId, SegmentDescriptor) **[NEW]**
- `sha2` (if hashing moved; currently 0.11.0 in storage) **[CONDITIONAL]**

### What Stays in `andromeda-storage` (75 LOC)

**Storage-specific facade:**
1. `pub mod manifest` (re-export point)
2. Module docstring explaining storage/manifest separation
3. Delegated reexports:
   ```rust
   pub use andromeda_manifest::{
       DatabaseManifest,
       StorageFormatManifest,
       ColdSegmentPublicationPlan,
       DatabaseSnapshotPublication,
       SnapshotAvailabilityContract,
       SnapshotSegmentReference,
       validate_cold_segment_publication_boundary,
       DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS,
   };
   pub use andromeda_manifest::ManifestDurabilityBoundary;
   ```
4. Storage-specific integration (if any; currently none—manifest is domain-pure)

### Boundary Rationale (C5 Gate 1: Design Correctness)

**Why manifest deserves its own crate:**
1. **Durability Boundary:** Manifest represents the durable checkpoint proof. It must be recoverable and verifiable independent of storage internals (page formats, WAL codec).
2. **Mission-Critical Invariants:** C5 requires that visible commits never precede durable WAL. Manifest lock-step with WAL durability is a gate requirement.
3. **Cross-Crate Use:** Recovery module depends on manifest for crash-recovery matrix. Separation prevents circular imports.
4. **Schema Stability:** Manifest format is versioned (database_id, manifest_version, snapshot_id, CRCs). Changes must be auditable and backward-compatible.
5. **Testability:** Crash/recovery validation must be standalone. Manifest crate can have fuzz tests, property tests, and miri checks without pulling in page store or WAL codec.

---

## Atomicity Guarantees & C5 Compliance

### Snapshot/Switch/Truncate Atomic Operations

**Current Invariants (must be preserved):**

1. **Manifest Atomic Switch**
   ```rust
   pub fn validate_manifest_atomic_switch(
       manifest_checkpoint_lsn: Lsn,
       wal_durable_lsn: Lsn,
       wal_checkpoint_lsn: Lsn,
   ) -> Result<()>
   ```
   - Manifest checkpoint ≤ WAL checkpoint ≤ WAL durable
   - Bootstrap manifest (LSN=0) conflicts with non-zero WAL evidence
   - **Consequence:** Never visible without durable WAL coverage.

2. **Snapshot Publication Contract**
   - Must have ≥1 snapshot segment available
   - Published snapshot must remain in available set
   - Snapshot id must match manifest snapshot_id
   - All segment descriptor hashes must be non-zero
   - **Consequence:** Recovery cannot start if no snapshot exists.

3. **Cold Publication Boundary**
   - Immutable after publication to cold store
   - GPU must be disabled (C5: no GPU in commit path)
   - Background maintenance pipeline only
   - Cold-path IO budget required
   - **Consequence:** Cold segments cannot re-enter hot pipeline.

4. **Format Manifest Versioning**
   - Format fingerprints must be ordered (no duplicates)
   - Fingerprint hash computed from all fields (including database_id, manifest_version)
   - Version fields never zero
   - **Consequence:** Storage format changes are detectable and versionable.

### C5 Recovery Floor Proof

**Durable Evidence Chain:**
```
Manifest.base_checkpoint_lsn     → WAL checkpoint boundary
Manifest.required_wal_start_lsn  → Recovery floor (minimum WAL required)
Manifest.previous_manifest_hash  → Previous manifest identification (audit trail)
Manifest.manifest_crc            → Durable bytes corruption detection
```

**Extraction Impact:**
- ✅ No change to proof structure (manifest is pure contract)
- ✅ Recovery reads manifest boundary from andromeda-manifest crate
- ✅ WAL durability proof (andromeda-wal) remains independent
- ✅ Crash validation tests must verify LSN ordering before extraction

---

## Test Requirements (Gate 0-3)

### Gate 0: Module Extraction (Compile, No Regressions)

**Checklist:**
- [ ] Move all manifest impl code to `andromeda-manifest` crate
- [ ] Update all imports in `andromeda-storage` to reexport
- [ ] Verify `cargo build --workspace` succeeds
- [ ] Verify all 24 storage test files still find manifest types
- [ ] No new clippy warnings
- [ ] No unsafe code introduced

**Command:**
```bash
cargo build --workspace
cargo test --lib manifest
cargo test -p andromeda-storage --test '*manifest*'
```

### Gate 1: Design Correctness (Boundary Contracts)

**Checklist:**
- [ ] ManifestDurabilityBoundary validates LSN ordering
  - Test: manifest_exposes_recovery_floor
  - Test: manifest_rejects_wal_start_before_checkpoint
  
- [ ] StorageFormatManifest versioning is immutable
  - Test: storage_format_manifest_rejects_hash_mismatch_and_duplicates
  - Test: fingerprint duplicates rejected
  
- [ ] Snapshot publication requires segments and matching manifest
  - Test: snapshot_publication_requires_segments_and_matching_manifest
  - Test: snapshot_availability_requires_one_valid_snapshot_remaining
  
- [ ] Cold publication placement is restricted
  - Test: validate_cold_segment_publication_boundary (coverage)
  - Test: GPU must be disabled in cold publication

**Command:**
```bash
cargo test -p andromeda-manifest
cargo test -p andromeda-storage -p andromeda-manifest -- --include-ignored
```

### Gate 2: Crash/Recovery Validation (C5 Required)

**Checklist:**
- [ ] Manifest recovery floor is never violated
  - Test: manifest_atomic_switch_validates_lsn_ordering
  - Run: andromeda-recovery crash_recovery_matrix tests
  
- [ ] Recovery floor proof survives durable WAL truncation
  - New test: recovery_can_start_at_required_wal_start_lsn
  - New test: recovery_cannot_start_before_required_wal_start_lsn
  
- [ ] Snapshot publication validation prevents orphaned segments
  - New test: snapshot_publication_prevents_segment_orphaning
  
- [ ] Format manifest hash is tamper-proof
  - New test: format_manifest_hash_detects_field_mutation

**Commands:**
```bash
cargo test -p andromeda-recovery crash_recovery_matrix
cargo test -p andromeda-manifest --test '*recovery*' -- --include-ignored
```

### Gate 3: Integration & Observability (C5 Full Gate)

**Checklist:**
- [ ] ManifestTrace carries WAL floor evidence in decision trace
  - Test: manifest_validation_trace_carries_queryable_catalog_and_wal_floor
  - Verify: EventEnvelope wraps ManifestTrace correctly
  
- [ ] Manifest events observable through andromeda-observe
  - Test: ManifestTrace serialization preserves required_wal_start_lsn
  - Test: ManifestEventKind::Validation logged on startup
  
- [ ] Recovery startup uses manifest boundary from andromeda-manifest
  - Integration test: fast_start_from_manifest_and_scan validates floor
  - Integration test: forensic_start_from_manifest_and_scan rejects bad floor

**Commands:**
```bash
cargo test -p andromeda-manifest manifest_validation_trace
cargo test -p andromeda-recovery --test '*recovery*' -- --include-ignored
cargo test -p andromeda-storage --test 'crash_recovery_impl' -- --include-ignored
```

---

## Reexport Strategy

### Layer 1: Boundary Crate (`andromeda-manifest`)

**Public API (all marked `pub`):**
```rust
// Durability boundary (existing)
pub struct ManifestDurabilityBoundary { … }
pub fn validate_manifest_atomic_switch(…) -> Result<()>

// Storage-specific contracts (new)
pub struct DatabaseManifest { … }
pub struct StorageFormatManifest { … }
pub struct ColdSegmentPublicationPlan { … }
pub struct DatabaseSnapshotPublication { … }
pub struct SnapshotSegmentReference { … }
pub struct SnapshotAvailabilityContract { … }

// Constants
pub const DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS: […];

// Validations
pub fn validate_cold_segment_publication_boundary(…) -> Result<()>

// Error helper (pub(crate) or moved to andromeda-core?)
pub(crate) fn storage_error(…) -> AndromedaError
```

### Layer 2: Storage Reexport (`andromeda-storage`)

**Current:** `pub use andromeda_manifest::ManifestDurabilityBoundary;` + all `manifest::*` types

**After extraction:**
```rust
// manifest.rs becomes a thin facade module
pub use andromeda_manifest::{
    ColdSegmentPublicationPlan,
    DatabaseManifest,
    DatabaseSnapshotPublication,
    ManifestDurabilityBoundary,
    SnapshotAvailabilityContract,
    SnapshotSegmentReference,
    StorageFormatManifest,
    validate_cold_segment_publication_boundary,
    DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS,
};

// lib.rs continues to export:
pub use manifest::*;
```

**Backward Compatibility:** 100% maintained. All existing consumers of `andromeda_storage::manifest::*` still work.

### Layer 3: Direct Use in Recovery (`andromeda-recovery`)

**Current:** Already imports `andromeda_manifest::ManifestDurabilityBoundary`

**After extraction:** No change needed (already correctly scoped to manifest crate).

---

## Dependencies Map

### New Dependencies for `andromeda-manifest`

| Dependency | Version | Purpose | Already in storage? |
|------------|---------|---------|---------------------|
| `andromeda-core` | workspace | Error, Result, CatalogVersion | ✅ Yes |
| `andromeda-wal` | workspace | Lsn type | ✅ Yes |
| `andromeda-observe` | workspace | ManifestEventKind, TraceId, ManifestTrace | ✅ Yes |
| `andromeda-segment` | workspace | SegmentId, SegmentDescriptor | ✅ Yes |
| `sha2` | 0.11.0 | Hash computation (conditional) | ✅ Yes |

**Decision:** All dependencies already in `andromeda-storage` → no new external crates needed.

### Dependency Reduction in `andromeda-storage`

**Current:** 13 direct dependencies

**After manifest extraction:**
- `andromeda-manifest` becomes explicit dependency (already there)
- No change to dependency count (manifest dependencies are subset of storage)
- Theoretical: Could remove `sha2` from storage if hash moved entirely to manifest
  - But: Keep in storage for backward compat (may have other uses)

---

## Risk Assessment

### Extraction Risks (Medium → Low with validation)

| Risk | Severity | Mitigation | Gate |
|------|----------|-----------|------|
| **LSN ordering invariant violation** | 🔴 Critical | Existing tests validate; add crash injection test | 2 |
| **CRC collision (manifest corruption not detected)** | 🟡 High | Hash function is deterministic XorShift; add fuzz test | 2 |
| **Snapshot orphaning (segment loss)** | 🟡 High | Publication requires segment validation; add integration test | 2 |
| **GPU leakage into cold publication** | 🟡 High | Validation rejects gpu_enabled; already in tests | 1 |
| **Format fingerprint downgrade** | 🟠 Medium | Version comparison is strict; cannot downgrade | 1 |
| **Recovery floor proof invalidated** | 🔴 Critical | ManifestDurabilityBoundary proof unchanged; tests before move | 2 |
| **Circular import (storage → manifest → storage)** | 🟠 Medium | Manifest is pure contract (no storage imports); design check | 0 |
| **Integration test breakage** | 🟠 Medium | 24 tests verified in Gate 0; reexport facade ensures compat | 0 |

### Confidence Level: **HIGH** (80%+)

**Why:**
1. Manifest is already a pure contract (no circular imports)
2. Existing unit tests are comprehensive (recovery floor, snapshot, format validation)
3. Reexport strategy maintains 100% backward compatibility
4. No structural changes to invariants; only crate boundary movement

---

## Ready-to-Extract Checklist

### Pre-Extraction (Week 3 Final)

- [ ] **Code Review:** Manifest module code reviewed for extraction readiness
- [ ] **Test Coverage:** All 147 unit tests in manifest/tests.rs passing
- [ ] **Dependency Analysis:** No circular imports detected
- [ ] **Documentation:** C5 invariants documented in lib.rs docstring
- [ ] **Cargo.toml Draft:** Dependencies locked, version aligned with workspace

### Extraction Steps (Week 4)

1. [ ] **Copy Implementation**
   - Move manifest/*.rs files to andromeda-manifest/src/
   - Move tests.rs to andromeda-manifest/tests/ or src/lib.rs tests section
   - Update module paths (remove `super::` prefixes, use full paths)

2. [ ] **Update Dependencies**
   - Add `andromeda-observe`, `andromeda-segment` to manifest Cargo.toml
   - Add `sha2` if not already present

3. [ ] **Fix Imports**
   - Replace `crate::` with module paths in manifest impl
   - Import `Lsn` from `andromeda_wal`
   - Import error builders from `andromeda_core`

4. [ ] **Reexport in Storage**
   - Replace manifest module with reexport-only manifest.rs (75 LOC)
   - Verify `pub use andromeda_manifest::*` in lib.rs

5. [ ] **Test**
   - Run Gate 0: Full build and unit tests
   - Run Gate 1: Boundary contract validation
   - Run Gate 2: Crash recovery validation
   - Run Gate 3: Integration tests

6. [ ] **Documentation**
   - Update manifest_extraction_status.md
   - Document any compatibility notes

### Post-Extraction (Week 4 Final)

- [ ] **Validation:** All 4 gates passing
- [ ] **Performance:** No regression in recovery startup time
- [ ] **Observability:** ManifestTrace events logged correctly
- [ ] **Release Ready:** Manifest crate ready for future standalone publication

---

## Lines of Code Estimate

### Movement Summary

| Component | Current LOC | After Extract | Destination |
|-----------|------------|----------------|-------------|
| manifest.rs | 75 | 5-10 (reexport facade) | storage |
| cold_publication.rs | 93 | 93 | manifest |
| format.rs | 92 | 92 | manifest |
| hash.rs | 54 | 54 | manifest |
| snapshot.rs | 91 | 91 | manifest |
| tests.rs | 147 | 147 | manifest |
| **Subtotal** | **552** | **420 LOC moved, 75 LOC reexported** | |
| manifest/lib.rs (current) | 132 | 530-550 (includes impl) | manifest |
| **Grand Total** | **684** | **600-620 LOC in manifest, 75 in storage facade** | |

**Net effect:**
- `andromeda-manifest` grows from 132 LOC to ~550 LOC (4.2x)
- `andromeda-storage` shrinks ~420 LOC (replaced with thin reexport)
- All code remains in workspace (no duplication or loss)

---

## Open Questions & Decisions

### Decision: Error Types

**Question:** Should `storage_error()` helper move to manifest or stay in storage?

**Options:**
1. Move to manifest (breaks pure domain assumption if storage-specific)
2. Keep in storage, import via storage::manifest
3. Move to andromeda-core (generic storage error builder)

**Recommendation:** Option 3 (move to andromeda-core if not already there). Manifest should not depend on storage-specific error paths.

**Status:** ✅ Already in andromeda-core (`AndromedaErrorKind::Storage`)

### Decision: Observer Integration

**Question:** ManifestTrace is part of manifest, but requires andromeda-observe. Is this acceptable?

**Answer:** ✅ YES. ManifestTrace is observability metadata, not core contract. Acceptable dependency per C5 doctrine (observability is non-critical path; can be swapped).

### Decision: Segment Dependencies

**Question:** Manifest imports `SegmentDescriptor` and `SegmentId` from andromeda-segment. Does this couple manifest to segment layer?

**Answer:** ✅ Acceptable (expected coupling). Snapshots are collections of immutable segments. Segment identity (ID, descriptor hash) is part of snapshot publication. No circular import risk.

---

## Success Criteria

### Code Quality (Gate 0)
- ✅ `cargo fmt --all --check` passes
- ✅ `cargo clippy --workspace -- -D warnings` passes
- ✅ `cargo audit` passes (no vulnerabilities)

### Correctness (Gate 1)
- ✅ All existing unit tests pass (tests.rs)
- ✅ No test behavior change (only crate location)
- ✅ Backward-compatible reexports (all old paths work)

### Recovery Safety (Gate 2)
- ✅ Manifest recovery floor proof validated before/after move
- ✅ Crash injection tests pass (manifest integrity maintained)
- ✅ Durable WAL coverage proofs unchanged

### Integration (Gate 3)
- ✅ All 24 storage test files compile and pass
- ✅ Recovery module integration tests pass
- ✅ ManifestTrace observability events logged correctly

---

## Appendix: File Structure After Extraction

```
crates/
├── andromeda-manifest/
│   ├── Cargo.toml (updated: +observe, +segment)
│   ├── README.md (updated: implementation scope)
│   └── src/
│       ├── lib.rs (existing boundary + new impl)
│       ├── cold_publication.rs (moved)
│       ├── format.rs (moved)
│       ├── hash.rs (moved)
│       ├── snapshot.rs (moved)
│       └── (tests inline or in tests/)
│
├── andromeda-storage/
│   ├── Cargo.toml (no change to deps)
│   ├── src/
│   │   ├── lib.rs (no change)
│   │   ├── manifest.rs (→ thin reexport facade, 5-10 LOC)
│   │   ├── (other modules unchanged)
│   │   └── manifest/ (REMOVED, code moved to andromeda-manifest)
│   └── tests/ (unchanged, imports via pub use manifest::*)
│
└── andromeda-recovery/
    ├── Cargo.toml (no change)
    └── src/ (already imports from andromeda-manifest, no change)
```

---

## References

- **C5 Gate Document:** `docs/architecture/C5_GATES.md`
- **Recovery Module:** `crates/andromeda-recovery/README.md`
- **Storage Architecture:** `crates/andromeda-storage/README.md`
- **Manifest Test Artifacts:** `MANIFEST_TEST_ARTIFACTS.md`
- **Crash Recovery Matrix:** `crates/andromeda-recovery/src/crash_recovery_matrix.rs`

---

## Sign-Off

**Prepared by:** Codex Analysis  
**Review Status:** Ready for Week 4 execution  
**Next Action:** Proceed with Gate 0 extraction (compile validation)
