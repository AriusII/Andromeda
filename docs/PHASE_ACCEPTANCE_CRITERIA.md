# Phase Acceptance Criteria

**Generated**: 2026-05-08  
**Purpose**: Define measurable gates for each restructuring phase  
**Scope**: Phases 0-7, success metrics per phase

## Phase 0: Baseline Establishment (Weeks 1-2)

### Acceptance Criteria

- ✅ Branch `architecture/workspace-restructure-2026` created
- ✅ All 12 Phase 0 documents complete and reviewed
- ✅ `cargo metadata` exported to `target/cargo-metadata.json`
- ✅ No breaking changes to public API
- ✅ All existing tests still pass
- ✅ All docstrings still compile (`cargo doc --no-deps`)
- ✅ Baseline SHAs recorded for all crates

### Gates

```bash
# Prerequisite: All Phase 0 docs exist
test -f docs/WORKSPACE_RESTRUCTURE_BASELINE_2026.md || exit 1
test -f docs/architecture/CARGO_DEPENDENCY_MATRIX.md || exit 1
test -f docs/MODULES_INVENTORY.md || exit 1
test -f docs/TEST_COVERAGE_MATRIX.md || exit 1
test -f docs/NAMING_DEBT_SCAN.md || exit 1
test -f docs/DOCUMENTATION_DEBT_SCAN.md || exit 1
test -f docs/CRITICALITY_MATRIX.md || exit 1
test -f docs/REEXPORT_MIGRATION_POLICY.md || exit 1
test -f docs/MSRV_POLICY.md || exit 1
test -f docs/adr/OPEN_DECISIONS.md || exit 1
test -f docs/PHASE_ACCEPTANCE_CRITERIA.md || exit 1

# Verify baseline state
cargo test --workspace --all-targets
cargo check --workspace --all-features
cargo clippy --workspace -- -D warnings
cargo doc --no-deps --workspace
```

### Sign-Off

- Architecture team reviews all 12 documents
- No breaking API changes detected
- Branch protected, ready for Phase 1

---

## Phase 1: Extract Core Components (Weeks 3-4)

### Scope

**Extract**: `andromeda-wal`, `andromeda-recovery`

**Reexport**: From original locations to maintain API

### Acceptance Criteria

#### Crate Extraction

- ✅ `andromeda-wal` compiles as standalone crate
- ✅ `andromeda-recovery` compiles as standalone crate
- ✅ All C5 tests pass in new crates
- ✅ Reexports added to original locations
- ✅ No breaking changes to public API
- ✅ `cargo tree` shows no circular dependencies

#### Testing

- ✅ All unit tests pass in new crates
- ✅ All integration tests pass
- ✅ C5 crash recovery matrix passes
- ✅ Fuzz targets execute without panics (1-hour runs)
- ✅ Cargo semver check passes

#### Code Quality

- ✅ `cargo fmt --check` passes
- ✅ `cargo clippy --all-targets -- -D warnings` passes
- ✅ `cargo deny check` passes (no forbidden deps)
- ✅ All C5 rustdoc complete (zero missing docs)

### Gates

```bash
# Extract crates
cargo check --workspace --all-targets

# Test C5 paths
cargo test --package andromeda-wal crash_recovery_matrix
cargo test --package andromeda-recovery --all-features

# Verify API stability
cargo semver-checks --baseline v0.65.0

# Run fuzz targets
cargo fuzz run frame_codec_no_panic -- -max_total_time=60
cargo fuzz run storage_wal_record_roundtrip -- -max_total_time=60

# Verify no cycles
cargo tree --all-features --package andromeda-wal
cargo tree --all-features --package andromeda-recovery
```

### Artifacts

- Extracted crates in `crates/andromeda-wal/`, `crates/andromeda-recovery/`
- Reexports in original locations
- Phase 1 summary report

### Sign-Off

- Storage team approves extraction
- All gates pass
- Ready for Phase 2

---

## Phase 2: Extract Storage (Weeks 5-6)

### Scope

**Extract**: `andromeda-buffer-pool`, `andromeda-backup`

**Reexport**: From `andromeda-storage`

### Acceptance Criteria

#### Crate Extraction

- ✅ `andromeda-buffer-pool` standalone
- ✅ `andromeda-backup` standalone
- ✅ `andromeda-storage` no longer contains moved code
- ✅ Reexports preserve API
- ✅ No circular dependencies

#### C5 Tests

- ✅ Recovery tests still pass
- ✅ Backup/restore integration tests pass
- ✅ Buffer pool stress tests pass
- ✅ Crash recovery matrix passes

#### Code Quality

- ✅ All C4+ rustdoc complete
- ✅ `cargo clippy` passes
- ✅ `cargo deny` passes
- ✅ No forbidden dependencies (no gRPC, no dynamic SQL)

### Gates

```bash
# Extract and verify
cargo check --workspace --all-targets
cargo clippy --workspace -- -D warnings

# Run C5 tests
cargo test crash_recovery_matrix
cargo test backup_restore_integration
cargo test buffer_pool_stress

# Verify new crate boundaries
cargo tree --package andromeda-storage | grep -E "andromeda-wal|andromeda-recovery|andromeda-buffer-pool|andromeda-backup"
```

### Artifacts

- `crates/andromeda-buffer-pool/`, `crates/andromeda-backup/`
- Reexports from `andromeda-storage`
- Phase 2 summary report

### Sign-Off

- Storage team approves
- All tests pass
- Ready for Phase 3

---

## Phase 3: Extract Catalog (Weeks 7-8)

### Scope

**Extract**: `andromeda-procedure-store`, `andromeda-catalog-store` improvements

**Reexport**: From `andromeda-catalog`

### Acceptance Criteria

- ✅ Procedure registry separable
- ✅ Catalog storage layer isolated
- ✅ DefinitionBatch tests pass (C5)
- ✅ Catalog consistency verified
- ✅ Reexports preserve API

### Gates

```bash
cargo test --package andromeda-catalog batch::*
cargo test --package andromeda-procedure-store --all-features
cargo clippy --workspace -- -D warnings
```

---

## Phase 4: Extract Execution (Weeks 9-10)

### Scope

**Extract**: Execution runtime components

**Reexport**: From `andromeda-exec`

### Acceptance Criteria

- ✅ Execution engine compiles
- ✅ All dispatcher tests pass
- ✅ Security gates functional
- ✅ Audit trail tests pass
- ✅ No broken references

### Gates

```bash
cargo test --package andromeda-exec dispatch::*
cargo test --package andromeda-security surface_gate::*
```

---

## Phase 5: Extract RPC (Weeks 11-12)

### Scope

**Extract**: RPC protocol layer (if needed)

**Verify**: QUIC-only requirement maintained

### Acceptance Criteria

- ✅ No gRPC dependencies present
- ✅ QUIC protocol tests pass
- ✅ RPC routing correct
- ✅ All 23 fuzz targets pass

### Gates

```bash
# No gRPC
grep -r "grpc\|tonic" crates --include="Cargo.toml" && exit 1 || true

# QUIC tests
cargo test --package andromeda-quic --all-features
cargo fuzz run quic_zero_rtt_admission -- -max_total_time=60
```

---

## Phase 6: Extract Analytics (Weeks 13-14)

### Scope

**Extract**: Analytics, GPU (off-commit), statistics

**Verify**: GPU never touches commit path

### Acceptance Criteria

- ✅ Analytics crate standalone
- ✅ GPU code never runs in C5 paths
- ✅ Statistics accurate
- ✅ No CPU regressions

### Gates

```bash
# Verify GPU off-commit
grep -r "gpu\|GPU" crates/andromeda-wal --include="*.rs" && exit 1 || true
grep -r "gpu\|GPU" crates/andromeda-transaction --include="*.rs" && exit 1 || true

# Run statistics tests
cargo test --package andromeda-statistics --all-features
cargo test --package andromeda-analytics --all-features
```

---

## Phase 7: Consolidate & Cleanup (Weeks 15-16)

### Scope

**Remove**: Reexports (consumers have migrated)

**Cleanup**: Rename debt, optimize boundaries

### Acceptance Criteria

- ✅ Reexports safe to remove (zero usage)
- ✅ All consumers migrated to new paths
- ✅ Naming debt addressed
- ✅ Documentation complete

### Gates

```bash
# Verify no old imports in code
grep -r "use andromeda_storage::wal::" crates --include="*.rs" && exit 1 || true
grep -r "use andromeda_storage::recovery::" crates --include="*.rs" && exit 1 || true

# Verify reexports removable
cargo remove-reexports --dry-run --all

# Final testing
cargo test --workspace --all-features
cargo clippy --workspace -- -D warnings
```

---

## Per-Phase Gate Template

```bash
#!/bin/bash
set -e

PHASE=$1

echo "=== Phase $PHASE Acceptance Gates ==="

# 1. Compilation
cargo check --workspace --all-targets --all-features
echo "✅ Compilation"

# 2. Formatting
cargo fmt --all -- --check
echo "✅ Formatting"

# 3. Linting
cargo clippy --workspace --all-targets -- -D warnings
echo "✅ Linting"

# 4. Testing
cargo test --workspace --all-targets
echo "✅ All tests"

# 5. C5 Tests (if applicable)
if [ $PHASE -ge 1 ]; then
  cargo test crash_recovery_matrix
  echo "✅ C5 Crash Recovery Matrix"
fi

# 6. Dependency Audit
cargo deny check
echo "✅ Dependency audit"

# 7. Documentation
cargo doc --no-deps --workspace
echo "✅ Documentation"

# 8. API Stability
cargo semver-checks --baseline v0.65.0
echo "✅ Semver"

echo ""
echo "=== Phase $PHASE: ALL GATES PASSED ==="
```

---

## Release Quality Gates

### Pre-Release

- ✅ All tests pass
- ✅ All clippy checks pass
- ✅ Changelog updated
- ✅ Version bumped (semver)
- ✅ No deprecation warnings (in new code)

### Release

- ✅ Tag created
- ✅ Artifacts built
- ✅ Release notes published
- ✅ Migration guide provided (if breaking)

### Post-Release

- ✅ Regression monitoring (week 1)
- ✅ Consumer feedback (week 2+)
- ✅ Performance baseline established

---

## Metrics to Track

| Metric | Target | Tool |
|--------|--------|------|
| Test Coverage | ≥85% (C5), ≥75% (C4), ≥60% (C3) | `cargo tarpaulin` |
| Doc Coverage | 100% (C5), 90% (C4) | `cargo doc` |
| Clippy Warnings | 0 | `cargo clippy` |
| Dependency Bloat | TBD | `cargo tree` |
| Compile Time | TBD | `cargo build` |
| Binary Size | TBD | `cargo build --release` |
| Fuzz Coverage | TBD | `cargo fuzz coverage` |

---

## Sign-Off Process

After each phase:

1. **Technical Approval**: Lead of affected subsystem
2. **Architecture Approval**: Architecture team
3. **Release Approval**: Release manager
4. **Document Update**: Update WORKSPACE_RESTRUCTURE_BASELINE_2026.md

---

## Rollback Plan

If phase fails:

1. Revert commit(s) to last passing phase
2. Document root cause
3. Create task for rework
4. Retry after fixes

---

## Next Steps

1. Phase 0 complete (current) ✅
2. Schedule Phase 1 kick-off
3. Assign teams to each phase
4. Prepare Phase 1 design documents
5. Review acceptance criteria with teams

## References

- Criticality Matrix: `docs/CRITICALITY_MATRIX.md`
- Test Coverage: `docs/TEST_COVERAGE_MATRIX.md`
- Modules Inventory: `docs/MODULES_INVENTORY.md`
- Dependency Matrix: `docs/architecture/CARGO_DEPENDENCY_MATRIX.md`
- Reexport Policy: `docs/REEXPORT_MIGRATION_POLICY.md`
- Roadmap: `.codex/prompts/workspace-restructure-roadmap.md`
