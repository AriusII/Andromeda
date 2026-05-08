//! Catalog snapshot publication state owned by catalog-store.
//!
//! The catalog engine remains responsible for DefinitionBatch planning and
//! applying object mutations. This module owns the portable snapshot
//! publication state and durable-visibility gates.

use andromeda_types::CatalogVersion;

use crate::CatalogPublicationSemantics;

/// Receipt behavior required by snapshot visibility gates.
pub trait CatalogSnapshotReceipt {
    fn next_version(&self) -> CatalogVersion;
}

impl<DefinitionBatchId, DefinitionBatchSourceHash, DefinitionBatchDependencyGraphHash>
    CatalogSnapshotReceipt
    for crate::CatalogPublicationReceipt<
        DefinitionBatchId,
        DefinitionBatchSourceHash,
        DefinitionBatchDependencyGraphHash,
    >
{
    fn next_version(&self) -> CatalogVersion {
        self.next_version
    }
}

/// Indicates whether the most recently applied mutation plan is backed by
/// durable WAL evidence or is staged in memory only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSnapshotPublication<Receipt> {
    /// The applied state has not been flushed to durable storage.
    InMemoryOnly,
    /// The applied state is covered by a durable publication receipt.
    Durable(Receipt),
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

/// Durable-visibility state for the currently applied snapshot version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogSnapshotPublicationGate<Receipt> {
    pub version: CatalogVersion,
    pub publication: CatalogSnapshotPublication<Receipt>,
    pub last_durable_version: CatalogVersion,
}

impl<Receipt> CatalogSnapshotPublicationGate<Receipt>
where
    Receipt: CatalogSnapshotReceipt + Copy,
{
    /// Returns the externally visible catalog version.
    pub fn visible_version(self) -> CatalogVersion {
        self.last_durable_version
    }

    /// Returns `true` only when the currently applied snapshot state has
    /// durable publication evidence and no staged in-memory mutation on top.
    pub fn is_durably_published(self) -> bool {
        matches!(self.publication, CatalogSnapshotPublication::Durable(_))
            && self.version == self.last_durable_version
    }

    /// Returns the durable receipt for the currently applied state.
    pub fn visible_publication_receipt(self) -> Option<Receipt> {
        match self.publication {
            CatalogSnapshotPublication::Durable(receipt)
                if self.version == self.last_durable_version
                    && receipt.next_version() == self.last_durable_version =>
            {
                Some(receipt)
            }
            _ => None,
        }
    }

    /// Returns the staged in-memory version when applied state is ahead of
    /// durable visibility.
    pub fn staged_in_memory_version(self) -> Option<CatalogVersion> {
        if self.version.get() > self.last_durable_version.get() {
            Some(self.version)
        } else {
            None
        }
    }
}
