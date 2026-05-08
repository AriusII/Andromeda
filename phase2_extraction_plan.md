# Phase 2: Core Types Extraction Plan

**Status:** Analysis Complete  
**Date:** 2025-Q1  
**Scope:** Extract pure type primitives from `andromeda-core` into specialized crates  
**Objective:** Reduce andromeda-core facade complexity and enable narrower dependency paths

---

## Executive Summary

This plan outlines the extraction of pure type primitives currently housed in andromeda-core into dedicated, zero-cost abstraction crates. The foundation split is already 80% complete: `andromeda-types`, `andromeda-digest`, `andromeda-hardware`, `andromeda-time`, and `andromeda-resource` exist as independent crates. Phase 2 focuses on:

1. **Principal types extraction** (2,401 LOC) → candidate for new `andromeda-principal` crate
2. **Facade reduction** in andromeda-core (keeping only reexports)
3. **Dependency chain optimization** to remove transitive coupling
4. **Test migration** to preserve coverage during extraction

**Estimated effort:** 4-6 weeks (implementation, testing, migration)

---

## Current Foundation State

### Existing Specialized Crates

| Crate | LOC | Dependents | Status | Risk |
|-------|-----|-----------|--------|------|
| **andromeda-error** | 45 | 40+ | Stable | Zero |
| **andromeda-types** | 511 | 30 | Stable | Low |
| **andromeda-digest** | 272 | 13 | Stable | Low |
| **andromeda-hardware** | 686 | 9 | Stable | Low |
| **andromeda-time** | 250 | 5 | Stable | Low |
| **andromeda-resource** | 609 | 2 | Stable | Low |
| **andromeda-policy** | 97 | 1 | Stable | Low |
| **andromeda-security-contract** | 1,422 | N/A | External | Zero |

### andromeda-core Current State

| Metric | Value |
|--------|-------|
| **Total LOC** | ~2,700 |
| **Facade LOC** | 64 (lib.rs) |
| **Principal module LOC** | 2,401 |
| **Principal submodules** | 29 files |
| **Test files** | 12 files |
| **Total test LOC** | 1,400+ |
| **Current dependents** | 28 crates |

---

## Type Candidates (Priority Order)

### Priority Tier 1: Principal Identity System

**Candidate:** Extract principal module → `andromeda-principal`

**Module Structure:**
```
src/principal/
├── id.rs                    [PrincipalId, SessionId]
├── identity.rs              [Principal, UserPrincipal]
├── certificate.rs           [CertificateFingerprint]
├── certificate_identity.rs  [CertificateIdentity, CertificateIdentityStatus]
├── permission.rs            [Permission]
├── permission_set.rs        [PermissionSet]
├── role.rs                  [PrincipalRole]
├── session.rs               [SessionToken]
├── status.rs                [PrincipalStatus]
├── surface_scope.rs         [SurfaceScope]
├── contract.rs              [Projection to andromeda-security-contract]
└── registry/
    ├── authorization.rs     [PrincipalAuthorizationDecision, etc.]
    ├── binding_store.rs     [PrincipalBinding]
    ├── denial.rs            [PrincipalAuthorizationDenialReason]
    └── policy.rs            [PrincipalPolicyVersion, PrincipalRegistry]
```

**Public Exports (from lib.rs):**
```rust
pub use certificate::CertificateFingerprint;
pub use certificate_identity::{CertificateIdentity, CertificateIdentityStatus};
pub use id::PrincipalId;
pub use identity::{Principal, UserPrincipal};
pub use permission::Permission;
pub use permission_set::PermissionSet;
pub use registry::{
    PRINCIPAL_POLICY_EVIDENCE_VERSION,
    PrincipalAuthorizationDecision,
    PrincipalAuthorizationDenialReason,
    PrincipalAuthorizationEvaluationStage,
    PrincipalAuthorizationEvidence,
    PrincipalAuthorizationOutcome,
    PrincipalBinding,
    PrincipalPolicyEvidenceBinding,
    PrincipalPolicyVersion,
    PrincipalRegistry,
};
pub use role::PrincipalRole;
pub use session::SessionToken;
pub use status::PrincipalStatus;
pub use surface_scope::SurfaceScope;
```

**Dependencies:**
- `andromeda-error` (shared with all foundation types)
- `andromeda-digest` (fingerprinting certificates)
- `andromeda-types::RequestId, SessionId` (internal cross-reference)

**Extracted LOC:** 2,401 lines (29 files)

**Test Coverage:**
- `certificate_identity.rs` (334 lines) - Certificate parsing and validation
- `certificate_fields.rs` (embedded) - X.509 field extraction
- `ids.rs` (embedded) - Principal ID generation and format
- `permissions.rs` (embedded) - Permission set semantics
- `principal.rs` (embedded) - Principal identity lifecycle
- `scope.rs` (embedded) - Surface scope projection
- `session.rs` (embedded) - Session token handling
- **Total extracted tests:** ~1,200 LOC

**Risk Assessment:**
- **Zero unsafe code** - pure Rust, no native bindings
- **Immutable public types** - no interior mutation
- **No I/O** - pure computation
- **No concurrency** - no locks, atomics, or channels
- **Projection to security-contract** - unidirectional mapping (no cycle)
- **Risk level:** **Low**

**Migration Complexity:**
- 28 dependent crates must update imports from `andromeda_core::` to `andromeda_principal::`
- See Dependency Chain section for safe migration order

---

### Priority Tier 2: Codec Infrastructure (Future Extraction Candidates)

These crates are not yet extracted but should be considered for Phase 3:

#### `andromeda-codec` (if extracted)
**Current state:** Not analyzed in this plan; referenced in workspace  
**Candidate:** Extract codec trait + implementations → `andromeda-codec-primitives`  
**Rationale:** Codecs are pure type → binary transformations with no runtime dependencies

#### `andromeda-structured-object` (if extracted)
**Current state:** Not analyzed in this plan; referenced in workspace  
**Candidate:** Extract type definition → `andromeda-object-model`  
**Rationale:** Type descriptors could be narrower than current dependencies

---

## Dependency Map

### Current andromeda-core Facade Dependencies

```
andromeda-core (facade)
├── andromeda-error ✓
├── andromeda-types ✓
├── andromeda-digest ✓
├── andromeda-hardware ✓
├── andromeda-time ✓
├── andromeda-resource (UNUSED - remove in Phase 2A)
├── andromeda-policy (UNUSED - remove in Phase 2A)
└── andromeda-security-contract (for contract reference only)
```

### Proposed andromeda-principal Dependencies

```
andromeda-principal (new)
├── andromeda-error ✓ (from Cargo.toml)
├── andromeda-digest ✓ (for SHA256 fingerprints)
└── andromeda-types ✓ (for RequestId, SessionId)
```

### Reverse Dependency Analysis (28 crates depend on andromeda-core)

**Core components:**
- andromeda-admission
- andromeda-audit
- andromeda-backup
- andromeda-cli
- andromeda-exec
- andromeda-execution
- andromeda-execution-trace
- andromeda-hadr
- andromeda-iam ← **CRITICAL:** Will depend on andromeda-principal
- andromeda-locking
- andromeda-manifest
- andromeda-mvcc
- andromeda-observe
- andromeda-quic
- andromeda-quic-runtime-quinn
- andromeda-retry
- andromeda-rpc
- andromeda-rpc-codec
- andromeda-rpc-protocol
- andromeda-savepoint
- andromeda-security
- andromeda-segment
- andromeda-storage
- andromeda-storage-heap
- andromeda-transaction
- andromeda-transaction-log
- andromeda-tx
- andromeda-wal

**Migration order:**
1. Direct dependents of principal types (andromeda-iam, andromeda-security)
2. RPC layer (andromeda-rpc, andromeda-quic, andromeda-protocol)
3. Storage layer (andromeda-wal, andromeda-transaction, andromeda-segment)
4. Admin layer (andromeda-backup, andromeda-hadr, andromeda-manifest)
5. Everything else

---

## Test Requirements Per Candidate

### andromeda-principal Extraction

**Existing test files (1,200+ LOC):**
1. `certificate_identity.rs` (334 lines)
   - X.509 certificate parsing
   - Certificate identity lifecycle
   - Validity period checks
   - Constraint validation

2. `decision_reference_consistency.rs` (145 lines)
   - Authorization decision structure
   - Evidence chain validation
   - Outcome consistency checks

3. `denial_reasons.rs` (139 lines)
   - Denial reason canonicalization
   - Stage-to-reason mapping
   - Error code generation

4. `disabled_revoked_states.rs` (103 lines)
   - Principal status transitions
   - Certificate revocation semantics
   - Access denial during disabled/revoked

5. `policy_evidence.rs` (169 lines)
   - Policy version tracking
   - Evidence binding structure
   - Version compatibility

6. `principal_registry.rs` (112 lines)
   - Registry storage semantics
   - Binding lookups
   - Role assignment consistency

7. `resource_policy_gates.rs` (311 lines)
   - Resource budget enforcement
   - Gate level semantics
   - Admission decision boundaries

8. `principal_integration.rs` (15 lines)
   - Module integration check
   - Public API stability

**Test types to preserve:**
- Unit tests (certificate parsing, state machines)
- Contract tests (projection to security-contract)
- Integration tests (registry operations, policy evaluation)
- Property-based tests (if present - none currently identified)

**New tests to add:**
- Backward compatibility for `andromeda_core::*` reexports
- Cross-crate imports from andromeda-principal

**Test file count:** 12 files → migrate to andromeda-principal/tests/

---

## Facade Reduction Strategy

### Phase 2A: Hygiene Cleanup (Week 1)

**Remove unused reexports from andromeda-core/src/lib.rs:**

Currently andromeda-core reexports types from unused crates. These should be removed in this phase:

```rust
// REMOVE THESE (currently unused):
// pub use andromeda_resource::*;  // Not reexported in lib.rs, resource module unused
// pub use andromeda_policy::*;    // Not reexported in lib.rs, policy module unused
```

**Action:**
- [ ] Audit current lib.rs reexports
- [ ] Identify unused crates in Cargo.toml
- [ ] Remove them from dependencies and reexports
- [ ] Run `cargo check --all` to verify no breakage

**Outcome:** andromeda-core Cargo.toml shrinks by 2 dependencies

---

### Phase 2B: Principal Extraction (Weeks 2-3)

**Step 1: Create andromeda-principal crate**

```toml
# crates/andromeda-principal/Cargo.toml
[package]
name = "andromeda-principal"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish = false

[dependencies]
andromeda-digest.workspace = true
andromeda-error.workspace = true
andromeda-types.workspace = true

# NOTE: No dependency on andromeda-security-contract
# Projection is one-way: principal → security-contract only
# Security-contract remains independent
```

**Step 2: Move src/principal/ → andromeda-principal/src/**

- Move `src/principal/mod.rs` → `andromeda-principal/src/lib.rs`
- Preserve all 29 source files
- Preserve all 12 test files

**Step 3: Create compatibility facade in andromeda-core**

```rust
// crates/andromeda-core/src/lib.rs (after extraction)

/// Compatibility re-exports for crates migrating from andromeda_core::*
pub mod principal {
    pub use andromeda_principal::*;
}

// Direct reexports for backward compatibility (deprecated path)
pub use andromeda_principal::{
    CertificateFingerprint,
    CertificateIdentity,
    CertificateIdentityStatus,
    Permission,
    PermissionSet,
    Principal,
    PrincipalAuthorizationDecision,
    PrincipalAuthorizationDenialReason,
    PrincipalAuthorizationEvaluationStage,
    PrincipalAuthorizationEvidence,
    PrincipalAuthorizationOutcome,
    PrincipalBinding,
    PrincipalId,
    PrincipalPolicyEvidenceBinding,
    PrincipalPolicyVersion,
    PrincipalRegistry,
    PrincipalRole,
    PrincipalStatus,
    SessionToken,
    SurfaceScope,
    UserPrincipal,
    PRINCIPAL_POLICY_EVIDENCE_VERSION,
};
```

**Step 4: Update andromeda-core tests**

- Keep `foundation_facade_compat.rs` to verify compatibility
- Add tests for principal reexports
- Add tests for direct imports: `use andromeda_core::Principal;`

**Step 5: Migrate dependents (safe migration order)**

See Migration Execution section below.

---

### Phase 2C: Cleanup & Stabilization (Week 4)

**After all dependents migrated:**

```rust
// crates/andromeda-core/src/lib.rs (final)
// Keep compatibility reexports for 2 more releases, then deprecate

#[deprecated(
    since = "0.4.0",
    note = "import from andromeda_principal directly"
)]
pub use andromeda_principal::{Principal, PrincipalId, /*...*/};
```

---

## Implementation Order

### Order 1: Zero-Dependent Crates First

| Step | Crate | Action | Dependencies |
|------|-------|--------|--------------|
| 1 | andromeda-principal | Create, move principal module | andromeda-error, andromeda-types, andromeda-digest |
| 2 | andromeda-core | Reduce facade, add compat | → andromeda-principal |

### Order 2: Leaf Dependents → Root Dependents

Migration follows reverse topological sort:

1. **Leaf tier (direct principal users):**
   - andromeda-iam
   - andromeda-security

2. **Mid tier (systems using principal indirectly):**
   - andromeda-quic
   - andromeda-rpc
   - andromeda-rpc-codec
   - andromeda-rpc-protocol

3. **Core tier (storage/transaction):**
   - andromeda-transaction
   - andromeda-segment
   - andromeda-wal
   - andromeda-mvcc

4. **Admin tier:**
   - andromeda-backup
   - andromeda-hadr
   - andromeda-manifest
   - andromeda-observe

5. **Remaining (can import from either location):**
   - All others

---

## Dependency Migration Strategy

### Migration Pattern (for each dependent crate)

**Before:**
```rust
// src/lib.rs or src/*.rs
use andromeda_core::{Principal, PrincipalId, Permission};
```

**After (Option A - Recommended):**
```rust
// src/lib.rs or src/*.rs
use andromeda_principal::{Principal, PrincipalId, Permission};
```

**After (Option B - During Transition):**
```rust
// Both work during transition period (2 releases)
use andromeda_core::{Principal, PrincipalId, Permission};  // Deprecated
// OR
use andromeda_principal::{Principal, PrincipalId, Permission};  // Preferred
```

**Cargo.toml update pattern:**
```toml
# Before
[dependencies]
andromeda-core.workspace = true

# After
[dependencies]
andromeda-core.workspace = true  # Keep for facade compat during transition
andromeda-principal.workspace = true  # Add this
```

**Safe cutoff strategy:**
- Week 1-2: Migration window (both paths work)
- Week 3: Deprecation warnings in andromeda-core
- Release 0.4: Break old path (require direct imports)

---

## Facade Reduction Strategy Summary

### Current andromeda-core Facade

```
lib.rs: 64 lines

Public exports (reexport pattern):
├── From andromeda-error (3 types)
├── From andromeda-types (14 types)
├── From andromeda-digest (2 functions)
├── From andromeda-hardware (8 types)
├── From andromeda-time (4 types)
├── From andromeda-principal (16 types + 1 constant) ← EXTRACT
└── Compatibility modules:
    ├── policy:: (reexports hardware)
    └── digest:: (reexports digest)
```

### Post-Extraction andromeda-core Facade

```
lib.rs: ~80 lines (+ 12 more for compat)

Public exports (reexport pattern):
├── From andromeda-error (3 types)
├── From andromeda-types (14 types)
├── From andromeda-digest (2 functions)
├── From andromeda-hardware (8 types)
├── From andromeda-time (4 types)
├── From andromeda-principal (16 types + 1 constant) [COMPAT ONLY]
└── Compatibility modules:
    ├── policy:: (reexports hardware)
    ├── digest:: (reexports digest)
    └── principal:: (reexports andromeda-principal) [NEW]

Dependencies:
- BEFORE: 6 crates (error, types, digest, hardware, time, security-contract)
- AFTER: 7 crates (add andromeda-principal, keep security-contract reference)
- ACTUAL: 6 crates (remove unused resource, policy)
```

**Facade surface reduction:**
- Reduces direct imports to principal types by enabling narrower paths
- Enables crates to depend on andromeda-principal without pulling andromeda-core
- Enables andromeda-core to eventually become a pure compatibility shim

**Future trajectory:**
- Phase 3: Audit other extractable modules (codec, structured-object)
- Phase 4: Reduce core to compatibility facade only
- Phase 5+: Eventually deprecate andromeda-core entirely (long-term, 2+ releases)

---

## Risk Assessment

### Risk Matrix

| Component | Risk | Mitigation |
|-----------|------|-----------|
| **Principal extraction** | **Low** | No unsafe code, immutable types, pure computation |
| **Dependency cycles** | **Low** | andromeda-principal → andromeda-security-contract only (no reverse) |
| **Test coverage** | **Low** | All 12 test files migrate; coverage preserved |
| **Backward compatibility** | **Medium** | 2-release transition window with reexports |
| **28 dependent crates** | **Medium** | Automated migration script + staged rollout |
| **Security-contract projection** | **Low** | One-way mapping; already stable contract |

### Validation Gates

**Before extraction:**
- [ ] All 12 test files pass in andromeda-core
- [ ] foundation_facade_compat.rs passes
- [ ] principal_contract_projection.rs passes

**After creation of andromeda-principal:**
- [ ] All 12 test files pass in new crate
- [ ] `cargo check --all` passes
- [ ] No new warnings from clippy

**During migration:**
- [ ] Each dependent crate compiles with updated imports
- [ ] All tests in dependent crates still pass
- [ ] No performance regressions (compile time)

**Before release:**
- [ ] Backward compat shim tested in andromeda-core
- [ ] Deprecation warnings clear to users
- [ ] Migration guide published

---

## Line of Code (LOC) Summary

### Extraction Size Estimates

| Component | LOC | Files | Destination |
|-----------|-----|-------|-------------|
| **Principal module** | 2,401 | 29 | andromeda-principal/src |
| **Principal tests** | 1,200+ | 12 | andromeda-principal/tests |
| **Compat facade** | +30 | +1 | andromeda-core/src/lib.rs |
| **Total extracted** | **3,600+** | 42 | 2 crates |

### Effort Estimate by Phase

| Phase | Task | Effort | Duration |
|-------|------|--------|----------|
| **2A** | Hygiene cleanup | 2 days | Week 1 |
| **2B.1** | Create andromeda-principal | 3 days | Week 2 |
| **2B.2** | Migrate principal module | 2 days | Week 2 |
| **2B.3** | Migrate tests | 2 days | Week 2-3 |
| **2B.4** | Create compat facade | 2 days | Week 3 |
| **2B.5** | Migrate 28 dependents | 10 days | Week 3-4 |
| **2C** | Cleanup, testing, docs | 3 days | Week 4 |
| **Total** | | 24 days | 4 weeks |

---

## Reexport Strategy from Core

### Pattern: Compatibility During Transition

```rust
// crates/andromeda-core/src/lib.rs

#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Core

Foundation types for Andromeda crates: identifiers, errors, clocks,
Andromeda type descriptors, principal identity, and conservative
hardware/resource contracts.

This crate is a compatibility facade over the foundation split. Public
exports here remain stable while downstream crates migrate to narrower
dependencies.

## Migration Guide

Types have been moved to specialized crates:
- Principal types → [`andromeda_principal`]
- Type descriptors → [`andromeda_types`]
- Digest primitives → [`andromeda_digest`]
- Hardware profiles → [`andromeda_hardware`]
- Time abstractions → [`andromeda_time`]
- Error types → [`andromeda_error`]

Imports from this crate will continue to work through 0.4.x but are
deprecated. Update your Cargo.toml to depend directly on the relevant
specialized crate:

```toml
# Before (deprecated)
[dependencies]
andromeda-core.workspace = true

# After (recommended)
[dependencies]
andromeda-principal.workspace = true
andromeda-types.workspace = true
andromeda-hardware.workspace = true
# ... etc for what you actually use
```
"#]

// === Direct reexports (backward compat) ===

pub use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
pub use andromeda_types::{
    AbsencePolicy, CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId,
    DecimalType, FloatMode, FloatType, InvocationId, NamespaceId, ProcedureId, RequestId,
    ScalarType, SessionId, TextEncoding, TextType, TimestampType, TransactionId, TypeDescriptor,
};
pub use andromeda_hardware::{
    CpuCapabilityClass, CpuProfile, GpuExecutionPolicy, GpuProfile, HardwareArchitecture,
    HardwareProfile, PipelineClass, RamProfile, RamSectionBudget, RamSectionRole, ResourceBudget,
};
pub use andromeda_time::{Clock, EngineTimestamp, ManualClock, SystemClock};

// === NEW: Principal reexports ===
pub use andromeda_principal::{
    CertificateFingerprint, CertificateIdentity, CertificateIdentityStatus,
    PRINCIPAL_POLICY_EVIDENCE_VERSION, Permission, PermissionSet, Principal,
    PrincipalAuthorizationDecision, PrincipalAuthorizationDenialReason,
    PrincipalAuthorizationEvaluationStage, PrincipalAuthorizationEvidence,
    PrincipalAuthorizationOutcome, PrincipalBinding, PrincipalId,
    PrincipalPolicyEvidenceBinding, PrincipalPolicyVersion, PrincipalRegistry,
    PrincipalRole, PrincipalStatus, SessionToken, SurfaceScope, UserPrincipal,
};

// === Compatibility modules ===

pub mod digest {
    pub use andromeda_digest::{Sha256, sha256};
}

pub mod policy {
    pub use andromeda_hardware::policy::*;
}

pub mod principal {
    pub use andromeda_principal::*;
}
```

### Testing Compatibility

```rust
// crates/andromeda-core/tests/foundation_facade_compat.rs

#[test]
fn core_facade_preserves_principal_reexports() {
    // Both paths should work (new direct import + compat path)
    use andromeda_core::{Principal, PrincipalId};
    use andromeda_core::principal::{Permission, PermissionSet};
    
    let id = PrincipalId::new(1);
    let _principal = Principal::anonymous();
    let _perm = Permission::ReadContractMetadata;
}
```

---

## Success Criteria

### Phase 2 Completion Checklist

- [ ] **andromeda-principal crate created** with all 29 principal module files
- [ ] **All 12 principal tests pass** in new crate
- [ ] **Backward compat facade** added to andromeda-core
- [ ] **28 dependent crates migrated** (or using compat reexports)
- [ ] **No performance regressions** (compile time stable or improved)
- [ ] **No test failures** in dependent crates
- [ ] **Deprecation warnings visible** when using old path (optional for 0.3.x)
- [ ] **Documentation updated** with migration guide
- [ ] **Git history clean** with atomic commits per crate migration

### Phase 2A Quick Wins

- [ ] Remove unused `andromeda-resource`, `andromeda-policy` from andromeda-core Cargo.toml
- [ ] Update workspace Cargo.toml to register andromeda-principal if missing
- [ ] Add andromeda-principal to any shared test-support crates

---

## Next Steps (Phase 2B+)

1. **Week 1 (Phase 2A):** Hygiene cleanup
   - Audit andromeda-core Cargo.toml
   - Remove unused dependencies
   - Validate `cargo check --all` still passes

2. **Week 2 (Phase 2B.1-2):** Create andromeda-principal
   - Create crate structure
   - Move principal module
   - Run tests

3. **Week 3 (Phase 2B.3-4):** Migrate dependents (staged rollout)
   - Tier 1: andromeda-iam, andromeda-security
   - Tier 2: RPC layer
   - Tier 3: Storage layer
   - Verify each tier builds and tests pass

4. **Week 4 (Phase 2C):** Stabilization
   - Add compat facade to andromeda-core
   - Final validation across workspace
   - Documentation and migration guide

---

## Future Phases (Phase 3+)

### Phase 3: Codec Extraction (Post-Phase 2)
- Extract codec infrastructure (if extractable)
- Analyze structured-object module
- Plan specialized codec crates

### Phase 4: Facade Reduction
- Reduce andromeda-core to pure reexport layer
- Measure compile-time improvements
- Evaluate deprecation strategy

### Phase 5: Long-Term Deprecation
- Deprecate andromeda-core in 0.5.x (after 0.4.x stable)
- Direct users to specialized crates
- Remove andromeda-core in 2.0.0

---

## References

### Related Documentation
- `segment_extraction_plan.md` - Phase 1 (storage layer separation)
- `.codex/docs/` - Andromeda architecture
- `CONTRIBUTING.md` - Development guidelines
- `.github/instructions/rust.instructions.md` - Rust practices

### Test Files (to be migrated)
- `crates/andromeda-core/tests/certificate_identity.rs`
- `crates/andromeda-core/tests/decision_reference_consistency.rs`
- `crates/andromeda-core/tests/denial_reasons.rs`
- `crates/andromeda-core/tests/disabled_revoked_states.rs`
- `crates/andromeda-core/tests/foundation_facade_compat.rs`
- `crates/andromeda-core/tests/policy_evidence.rs`
- `crates/andromeda-core/tests/principal_contract_projection.rs`
- `crates/andromeda-core/tests/principal_registry.rs`
- `crates/andromeda-core/tests/resource_policy_gates.rs` (x2)
- `crates/andromeda-core/tests/principal_integration.rs`
- `crates/andromeda-core/tests/support/` (shared fixtures)

### Key Metrics
- **Principal module extraction:** 2,401 LOC → ~1 new crate
- **Test coverage:** 1,200+ LOC → preserved in andromeda-principal
- **Backward compatibility:** 2-release window
- **Risk level:** Low (pure types, no unsafe code, no I/O)
- **Dependent crates:** 28 (staged migration)

---

**Document Version:** 1.0  
**Last Updated:** 2025-Q1  
**Status:** Ready for Phase 2B Implementation
