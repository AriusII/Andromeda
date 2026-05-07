//! Snapshot-related value types and lifecycle descriptors.
//!
//! These types are shared across the snapshot sub-modules and are
//! re-exported from the module root.

use andromeda_types::CatalogVersion;

use crate::{CatalogPublicationReceipt, CatalogPublicationSemantics};

/// Indicates whether the most recently applied mutation plan is backed by
/// durable WAL evidence or is staged in memory only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSnapshotPublication {
    /// The applied state has not been flushed to durable storage.
    InMemoryOnly,
    /// The applied state is covered by a durable publication receipt.
    Durable(CatalogPublicationReceipt),
}

/// Coarse lifecycle state of a catalog object within a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogObjectLifecycleStatus {
    Active,
    Deprecated,
}

/// Lifecycle metadata recorded per catalog object in a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogObjectLifecycle {
    pub status: CatalogObjectLifecycleStatus,
    pub created_version: CatalogVersion,
    pub last_changed_version: CatalogVersion,
}

impl CatalogObjectLifecycle {
    /// Constructs an [`Active`](CatalogObjectLifecycleStatus::Active) lifecycle entry at
    /// `created_version`.
    pub fn active(created_version: CatalogVersion) -> Self {
        Self {
            status: CatalogObjectLifecycleStatus::Active,
            created_version,
            last_changed_version: created_version,
        }
    }

    /// Returns a new lifecycle record reflecting deprecation at `changed_version`.
    pub fn deprecated(self, changed_version: CatalogVersion) -> Self {
        Self {
            status: CatalogObjectLifecycleStatus::Deprecated,
            created_version: self.created_version,
            last_changed_version: changed_version,
        }
    }
}

/// Describes the outcome of applying a mutation plan to a catalog snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogSnapshotApplyReport {
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub applied_delta_count: usize,
    pub publication_semantics: CatalogPublicationSemantics,
    pub durable_publication_performed: bool,
}
