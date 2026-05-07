//! E7: SRPL DefinitionBatch Compatibility Verification
//!
//! This test suite verifies that the DefinitionBatch infrastructure is compatible with
//! the Alter Procedure (DEC-022) and Drop Procedure (DEC-023) operation semantics, even
//! though these operations are not yet implemented.
//!
//! # Scope
//!
//! This test verifies:
//! 1. **Create Procedure**: Validates version bump, ContractHash assignment, visibility
//! 2. **Alter Procedure Semantics**: Validates what Alter would require from batch infrastructure
//! 3. **Drop Procedure Semantics**: Validates what Drop would require from batch infrastructure
//! 4. **Replay Idempotency**: Validates that batch application is deterministic and safe
//!
//! # Exclusions
//!
//! - Alter/Drop are NOT implemented yet; tests use Create/Deprecate to verify infrastructure
//! - WAL integration (E4) is deferred; tests use dry-run only
//! - Actual IR lowering is not tested; only batch semantics
//!
//! # Decision Records Referenced
//!
//! - DEC-022: Alter Procedure Lifecycle Semantics (compatibility policy, version advancement)
//! - DEC-023: Drop Procedure Lifecycle Semantics (restrict rule, historical retention)

#[path = "batch_alter_drop_compat/common.rs"]
mod common;
#[path = "batch_alter_drop_compat/create_alter.rs"]
mod create_alter;
#[path = "batch_alter_drop_compat/drop_compat.rs"]
mod drop_compat;
#[path = "batch_alter_drop_compat/replay.rs"]
mod replay;
#[path = "batch_alter_drop_compat/validation.rs"]
mod validation;
