//! Storage layout domain facade.
//!
//! # Canonical ownership
//!
//! The root modules `crate::page`, `crate::extent`, `crate::segment`,
//! `crate::cold_store`, and `crate::placement` are the
//! single source of truth for storage layout types. This `layout` module is a
//! **documented facade** that groups those contracts by storage domain for
//! external callers and integration tests.
//!
//! Every item exported here is a `pub use` re-export of a root-owned
//! definition. Submodules MUST NOT introduce new type, struct, enum, trait,
//! free function, or const definitions. Adding a duplicate definition would
//! immediately break the facade-identity regression test in
//! `tests/layout_facade_invariants.rs` (the test relies on the re-exports
//! resolving to the same `TypeId` and being usable interchangeably with the
//! root path).
//!
//! New layout-domain types must be defined in their root module first and
//! then re-exported through the matching submodule below.

pub mod cold;
pub mod extent;
pub mod io_budget;
pub mod page;
pub mod placement;
pub mod segment;

pub use cold::*;
pub use extent::*;
pub use io_budget::*;
pub use page::*;
pub use placement::*;
pub use segment::*;
