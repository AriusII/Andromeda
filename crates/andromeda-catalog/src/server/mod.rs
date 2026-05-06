//! Catalog Server boundary traits and contracts.
//!
//! This module defines the trait boundaries for the Catalog Server component,
//! enabling remote clients to query procedure metadata, track catalog versions,
//! and subscribe to catalog changes without executing application traffic.
//!
//! ## Contract Overview
//!
//! The Catalog Server provides three primary operations:
//! 1. **Procedure Resolution**: Fetch procedure manifests by ID
//! 2. **Version Tracking**: Query current catalog version
//! 3. **Change Subscriptions**: Subscribe to catalog version updates
//!
//! ### Key Invariants
//!
//! - **No Ad Hoc SQL**: Catalog server queries only pre-authorized catalog views
//! - **LSN-Aware Invalidation**: Changes propagate via log sequence number (LSN) boundaries
//! - **Deterministic Hashing**: Procedure contracts use stable contract hashes
//! - **Error Observability**: All failures returned via `AndromedaResult<T>`
//! - **Remote-First Design**: Clients query metadata independent of application execution
//!
//! ## Usage Pattern
//!
//! ```ignore
//! use andromeda_catalog::server::{CatalogServerTrait, CatalogVersion};
//! use andromeda_core::ProcedureId;
//!
//! let server = MockCatalogServer::new();
//! let procedure = server.resolve_procedure(ProcedureId::new(1))?;
//! let version = server.get_catalog_version();
//! let mut subscription = server.subscribe_to_changes()?;
//! while let Some(change) = subscription.next_change().await {
//!     println!("Catalog updated to version: {}", change.new_version.get());
//! }
//! ```

mod mock;
mod model;
mod subscription;
mod traits;

pub use mock::*;
pub use model::*;
pub use subscription::*;
pub use traits::*;

#[cfg(test)]
mod tests;
