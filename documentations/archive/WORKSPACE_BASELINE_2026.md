# Workspace Restructure Baseline 2026

**Phase 0: Baseline Establishment & Inventory** (Weeks 1-2)

## Branch Information

- **Branch Name**: `architecture/workspace-restructure-2026`
- **Created From**: `codex/workspace-crate-restructure`
- **Baseline Commit SHA**: `1486f240d85b9dec29adf4736d4bbd15f2f1dbb8`
- **Baseline Date**: 2026-05-08 22:29:48 UTC+2
- **Protection Status**: ⚠️ Unprotected (will be protected after Phase 0 approval)
- **Documentation**: Roadmap in `.codex/prompts/workspace-restructure-roadmap.md`

## Baseline Metrics

| Metric | Value | Status |
|--------|-------|--------|
| Total Crates | 96 | ✅ Inventoried |
| Rust Files | 1,954 | ✅ Scanned |
| Expected Modules | ~400 | 📊 Inventory pending |
| Test Files | TBD | 📊 Mapping pending |
| Fuzz Targets | TBD | 📊 Cataloguing pending |
| Naming Debt Items | TBD | 📊 Scan pending |
| Documentation Debt Items | TBD | 📊 Scan pending |

## Phase 0 Deliverables Status

### ✅ Task 0.01: Create Restructuring Branch
- **Status**: COMPLETE
- **Output**: Branch `architecture/workspace-restructure-2026` created
- **Evidence**: Branch exists, clean state from baseline commit

### ⏳ Task 0.02: Export Cargo Metadata & Dependency Graph
- **Status**: IN PROGRESS
- **Output**: `target/cargo-metadata.json` (exported)
- **Output**: `docs/architecture/CARGO_DEPENDENCY_MATRIX.md` (pending)
- **Command**: `cargo metadata --format-version 1`

### ⏳ Task 0.03: List All Modules by Crate
- **Status**: IN PROGRESS
- **Crates Identified**: 96 crates
- **Output**: `docs/MODULES_INVENTORY.md` (pending)
- **Categories**: contract, storage, tx, rpc, security, stats, admin, tooling

### ⏳ Task 0.04: Map All Existing Tests
- **Status**: PENDING
- **Output**: `docs/TEST_COVERAGE_MATRIX.md` (pending)
- **Focus**: Unit/integration/property/fuzz/doc tests by module
- **C5 Tests**: WAL, recovery, commit, catalog, audit

### ⏳ Task 0.05: Map Existing Fuzz Targets
- **Status**: PENDING
- **Output**: `fuzz/README.md` (pending)
- **Focus**: All fuzz targets, seed corpus, reproducibility

### ⏳ Task 0.06: Scan Naming Debt
- **Status**: PENDING
- **Output**: `docs/NAMING_DEBT_SCAN.md` (pending)
- **Patterns**: v0, v1, v2, wave, vertical, legacy, old, tmp, helper, scaffold

### ⏳ Task 0.07: Scan Documentation Debt
- **Status**: PENDING
- **Output**: `docs/DOCUMENTATION_DEBT_SCAN.md` (pending)
- **Focus**: Stale rustdoc, over-commenting, historical context

### ⏳ Task 0.08: Define Criticality Matrix
- **Status**: PENDING
- **Output**: `docs/CRITICALITY_MATRIX.md` (pending)
- **Classes**: C0-C5 per module/crate

### ⏳ Task 0.09: Define Reexport Rules
- **Status**: PENDING
- **Output**: `docs/REEXPORT_MIGRATION_POLICY.md` (pending)
- **Rules**: pub use only, migration notes, no deprecate+remove

### ⏳ Task 0.10: Define MSRV Policy
- **Status**: PENDING
- **Output**: `docs/MSRV_POLICY.md` (pending)
- **Current**: rust-version = "1.85" (stable 2024)

### ⏳ Task 0.11: Create Open Decisions Registry
- **Status**: PENDING
- **Output**: `docs/adr/OPEN_DECISIONS.md` (pending)
- **Decisions**: Page size, ContractHash, CatalogVersion, GPU, optimizer

### ⏳ Task 0.12: Define Acceptance Format Per Phase
- **Status**: PENDING
- **Output**: `docs/PHASE_ACCEPTANCE_CRITERIA.md` (pending)
- **Gates**: Cargo commands, tests, non-regression checks, crates touched

## Non-Goals (Phase 0)

- ❌ Code changes (restructuring starts Phase 1)
- ❌ Naming remediations (identified for Phase 3+)
- ❌ Documentation rewrites (planned for Phase 2+)
- ❌ Test additions (Phase 2+)

## Critical Success Factors

All Phase 0 tasks must complete before Phase 1 begins:

1. ✅ Branch created and documented
2. ⏳ Metadata exported and analyzed
3. ⏳ Modules inventoried with move-map
4. ⏳ Tests mapped (C5 vs C3 vs C0)
5. ⏳ Fuzz targets catalogued
6. ⏳ Naming debt prioritized
7. ⏳ Documentation plan established
8. ⏳ Criticality matrix locked
9. ⏳ Reexport rules defined
10. ⏳ MSRV policy locked
11. ⏳ Open decisions tracked
12. ⏳ Acceptance criteria measurable

## Next Steps

1. Complete all 12 task outputs
2. Validate cargo metadata for cycles and hubs
3. Lock all Phase 0 documents
4. Gate approval: All outputs exist and complete
5. Begin Phase 1: Crate extraction (weeks 3-4)

## References

- Roadmap: `.codex/prompts/workspace-restructure-roadmap.md`
- Cargo Metadata: `target/cargo-metadata.json`
- Phase 1+ Plans: Documented in PHASE_ACCEPTANCE_CRITERIA.md
- Architecture: `docs/codex/architecture.md`
