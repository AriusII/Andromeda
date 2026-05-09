//! Storage undo integration contract tests.
//!
//! Pure recovery replay contracts have moved to `andromeda-recovery`. Storage
//! keeps only the undo builder bridge because `UndoChainsBuilder` remains
//! storage-owned during this extraction phase.

#[path = "recovery_completeness_contract/support.rs"]
mod support;

#[path = "recovery_completeness_contract/redo_undo.rs"]
mod redo_undo;
