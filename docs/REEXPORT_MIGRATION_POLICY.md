# Reexport Migration Policy

**Generated**: 2026-05-08  
**Purpose**: Preserve API stability during Phase 1-6 restructuring  
**Scope**: All extracted crates and moved modules

## Core Principle

**During extraction (Phase 1-6): Maintain binary compatibility via `pub use`.**

Public API must not break. Internal restructuring is invisible to consumers.

---

## Policy Rules

### Rule 1: Keep Reexports During Phases 1-6

When a module moves from Crate A to Crate B:

```rust
// In Crate A (original location) - KEEP until Phase 7+
pub use crate_b::ModuleName;  // ✅ Preserved

// NEW module in Crate B (new location) - ADD in Phase 1+
pub mod module_name { ... }   // ✅ New home
```

**Example**: Moving WAL from `andromeda-storage` to `andromeda-wal`

```toml
# Cargo.toml (andromeda-storage)
[dependencies]
andromeda-wal = { path = "../andromeda-wal" }

# src/lib.rs (andromeda-storage) - KEEP during Phase 1-6
pub use andromeda_wal as wal;  // ✅ Consumers see no change
pub use andromeda_wal::*;      // ✅ Re-export types too
```

---

### Rule 2: Use `pub use` Only (Zero Logic)

Reexports must contain **zero logic**. No forwarding functions, no wrappers.

```rust
// ✅ CORRECT: Zero logic, pure reexport
pub use crate::inner_module::*;
pub use crate::inner_module::TypeName;

// ❌ WRONG: Logic in reexport (forbidden!)
pub fn moved_function() {
    // This wrapper function is FORBIDDEN
    inner_moved_function()
}
```

**Why**: Reexports are temporary. Logic stays in the actual module.

---

### Rule 3: Document Each Reexport with Migration Note

Every reexport must include a rustdoc note:

```rust
/// Reexported from andromeda-wal for API stability.
///
/// **Migration Note**: This type has moved to `andromeda_wal::WriteAheadLog`.
/// Direct import recommended for new code:
/// ```ignore
/// use andromeda_wal::WriteAheadLog;  // Preferred
/// use andromeda_storage::wal::WriteAheadLog;  // Old, but still works
/// ```
///
/// This reexport will be removed in Phase 7 (Q3 2026).
pub use crate_b::WriteAheadLog;
```

---

### Rule 4: Never Deprecate + Remove in Same PR

Deprecation and removal must be **separate releases**:

1. **Release N**: Add `#[deprecated]` with link to ADR
2. **Release N+1**: Remove reexport (post-consumers migrate)

```rust
// Release 42 (Phase 2):
#[deprecated(since = "0.42.0", note = "Use andromeda_wal::WriteAheadLog")]
pub use crate_b::WriteAheadLog;

// Release 44+ (Phase 7, after 2 versions):
// ✅ OK to remove now
// (reexport removed)
```

---

### Rule 5: Removal Only After Imports Stabilized

Before removing reexports:

1. Verify all public consumers have updated imports
2. Internal code must use new paths (not reexports)
3. Document migration in MIGRATION.md
4. At least one release cycle of deprecation

---

## Migration Timeline

### Phase 1-2 (Weeks 3-6): Extract & Reexport

```rust
// andromeda-storage/src/lib.rs (original)
pub use andromeda_wal::*;  // ✅ Reexport to maintain API

// andromeda-wal/src/lib.rs (new)
pub mod write_ahead_log { ... }  // ✅ New home
pub use write_ahead_log::*;  // ✅ Publish from new crate
```

**Status**: API stable, internal structure changed

---

### Phase 3-6 (Weeks 7-20): Restructure (Keep Reexports)

Multiple extractions happen. Each maintains reexports.

```
andromeda-storage  
  └─ pub use andromeda_wal::*;      // ✅ Phase 1+
  └─ pub use andromeda_buffer_pool::*;  // ✅ Phase 2+
  └─ pub use andromeda_recovery::*;  // ✅ Phase 2+
  └─ pub use andromeda_backup::*;    // ✅ Phase 3+
```

**Status**: API stable during restructuring

---

### Phase 7 (Weeks 21+): Reexport Cleanup

Once consumers have migrated, begin selective removal:

1. **Add `#[deprecated]`** in Phase 7.0
2. **Wait one release cycle** (Phase 7.1+)
3. **Remove reexports** in Phase 7.2+

```rust
// Phase 7.0:
#[deprecated(since = "0.77.0", note = "Use andromeda_wal directly")]
pub use andromeda_wal::*;

// Phase 7.2+:
// ✅ Safe to remove - consumers have migrated
```

---

## Examples

### Example 1: Extract WAL Module

**Phase 1**: Move `andromeda-storage::wal` → new `andromeda-wal` crate

**Before (Phase 0)**:
```rust
// andromeda-storage/src/lib.rs
pub mod wal {
    pub struct Manager { ... }
    pub type LSN = u64;
}

// Consumer code:
use andromeda_storage::wal::Manager;
```

**During Phase 1-6**:
```rust
// andromeda-storage/src/lib.rs - KEEP REEXPORT
pub use andromeda_wal as wal;  // ✅ Maintains API

// andromeda-wal/src/lib.rs - NEW HOME
pub struct Manager { ... }
pub type LSN = u64;

// Consumer code (still works):
use andromeda_storage::wal::Manager;  // ✅ Still valid (uses reexport)
```

**Phase 7+** (optional cleanup):
```rust
// andromeda-storage/src/lib.rs - REMOVE REEXPORT
// Removed after consumers migrated to:
use andromeda_wal::Manager;  // Direct import
```

---

### Example 2: Extract Recovery Module

**Phase 2**: Move `andromeda-storage::recovery` → new `andromeda-recovery` crate

**During Phase 2-6**:
```rust
// andromeda-storage/src/lib.rs
pub use andromeda_recovery::*;

/// Reexported from andromeda-recovery for backward compatibility.
/// 
/// **Migration Note**: New code should import directly:
/// ```ignore
/// use andromeda_recovery::RecoveryPlanner;
/// ```
///
/// This reexport will be removed in Phase 7.
pub use andromeda_recovery::RecoveryPlanner;

// Consumer code (old path still works):
use andromeda_storage::recovery::RecoveryPlanner;

// Consumer code (new path recommended):
use andromeda_recovery::RecoveryPlanner;
```

---

### Example 3: Extract Buffer Pool

**Phase 2**: Move `andromeda-storage::buffer_pool` → new `andromeda-buffer-pool` crate

**During Phase 2-6**:
```rust
// andromeda-storage/src/lib.rs
pub use andromeda_buffer_pool::*;

// andromeda-storage/src/buffer_pool.rs (shim)
pub use andromeda_buffer_pool::*;

// Consumer code (via module reexport):
use andromeda_storage::buffer_pool::Manager;  // ✅ Still valid

// Consumer code (direct):
use andromeda_buffer_pool::Manager;  // ✅ Recommended
```

---

## Implementation Checklist

For each extraction:

- [ ] New crate created with `pub mod` structure
- [ ] All types published via `pub use` in new crate
- [ ] Original crate adds reexport: `pub use new_crate::*;`
- [ ] Original module marked with rustdoc migration note
- [ ] Tests updated to use new imports (internal only)
- [ ] No breaking changes to public API (verified via cargo-semver-check)
- [ ] Migration documented in PR description

---

## Cargo Semver Verification

Use `cargo-semver-checks` to prevent accidental API breaks:

```bash
# Before extraction:
cargo semver-checks --baseline v0.65.0

# Should pass (reexports don't break semver):
# ✅ New items added (reexports)
# ✅ No items removed from public surface
# ✅ No signature changes
```

---

## Deprecation Example

```rust
//! # Reexports for backward compatibility
//!
//! As of Phase 7, prefer importing directly from andromeda-wal:
//! ```ignore
//! use andromeda_wal::Manager;  // Direct import
//! ```

#[deprecated(
    since = "0.77.0",
    note = "This type has been moved to `andromeda_wal`. \
            Import directly: `use andromeda_wal::Manager;` \
            See https://github.com/.../issues/1234 for migration guide."
)]
pub use andromeda_wal::Manager;
```

---

## Consumer Guidance

**For internal Andromeda code**:
- Use new paths immediately after extraction
- No requirement to update old paths (but encouraged)

**For external consumers**:
- Old paths remain valid during Phase 1-7
- Deprecation warnings guide migration
- Recommended new path in error message

---

## Tools

### Verify Reexport Visibility

```bash
# Check what's exposed from reexports
cargo doc --package andromeda-storage --open
# Navigate to wal module - should see all types

# Check no items are hidden
cargo tree --all-features --package andromeda-storage | grep "pub use"
```

### Semver Checking

```bash
# Install cargo-semver-checks
cargo install cargo-semver-checks

# Run before and after extraction
cargo semver-checks --baseline v0.65.0
```

---

## Failure Modes & Recovery

### Failure: Reexport Not Updated

**Symptom**: `pub use` missing after extraction

```rust
// ❌ WRONG: Forgot to add reexport
pub mod inner_module { ... }
// WAL not reexported!
```

**Recovery**:
1. Add reexport: `pub use crate_b::*;`
2. Release as patch (0.65.1)
3. Notify consumers to upgrade

### Failure: Logic in Reexport

**Symptom**: Wrapper function in reexport module

```rust
// ❌ WRONG: Logic in reexport
pub mod wrapper {
    pub fn moved_fn() { ... }  // This is wrong!
}
```

**Recovery**:
1. Remove wrapper logic
2. Use pure `pub use` instead
3. Add comment: "Reexport only, no logic"

---

## Next Steps

1. **Phase 0**: Document policy ✅
2. **Phase 1**: Create reexports during first extraction
3. **Phase 2**: Verify no API breaks with cargo-semver-checks
4. **Phase 7**: Begin optional cleanup (after migration window)

## References

- Cargo Semver Checks: https://docs.rs/cargo-semver-checks/
- Rust API Guidelines: https://rust-api-guidelines.rs/
- Module Inventory: `docs/MODULES_INVENTORY.md`
- Cargo Dependency Matrix: `docs/architecture/CARGO_DEPENDENCY_MATRIX.md`
