use std::collections::{BTreeMap, BTreeSet};

use andromeda_types::{CatalogObjectId, CatalogVersion};

/// In-memory snapshot of catalog state at a point in recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    /// Current catalog version.
    pub catalog_version: CatalogVersion,
    /// Set of visible procedure IDs.
    pub procedure_ids: BTreeSet<CatalogObjectId>,
    /// Map of procedure ID to last known catalog version at which it was visible.
    pub procedure_versions: BTreeMap<CatalogObjectId, CatalogVersion>,
}

impl CatalogSnapshot {
    pub fn new(catalog_version: CatalogVersion) -> Self {
        Self {
            catalog_version,
            procedure_ids: BTreeSet::new(),
            procedure_versions: BTreeMap::new(),
        }
    }

    /// Returns the count of visible procedures.
    pub fn visible_procedure_count(&self) -> usize {
        self.procedure_ids.len()
    }
}
