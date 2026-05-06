use std::sync::Arc;

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ProcedureId,
};

use super::{
    CatalogChangeNotification, CatalogChangeSubscription, CatalogServerTrait,
    MockCatalogChangeSubscription, ProcedureManifest,
};

/// Mock implementation of `CatalogServerTrait` for testing and development.
///
/// This implementation maintains an in-memory store of procedure manifests
/// and change notifications, suitable for unit testing and contract validation.
#[derive(Debug, Clone)]
pub struct MockCatalogServer {
    procedures: Arc<std::sync::Mutex<std::collections::HashMap<ProcedureId, ProcedureManifest>>>,
    current_version: Arc<std::sync::Mutex<CatalogVersion>>,
    changes: Arc<std::sync::Mutex<Vec<CatalogChangeNotification>>>,
}

impl MockCatalogServer {
    /// Create a new mock catalog server.
    pub fn new() -> Self {
        Self {
            procedures: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            current_version: Arc::new(std::sync::Mutex::new(CatalogVersion::new(1))),
            changes: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Register a procedure manifest in the mock server.
    pub fn register_procedure(&self, manifest: ProcedureManifest) -> AndromedaResult<()> {
        manifest.validate()?;
        let mut procedures = self.procedures.lock().unwrap();
        procedures.insert(manifest.procedure_id, manifest);
        Ok(())
    }

    /// Advance the catalog version and emit a change notification.
    pub fn advance_catalog_version(&self, invalidation_lsn: u64) -> AndromedaResult<()> {
        let mut version = self.current_version.lock().unwrap();
        let previous = *version;
        *version = CatalogVersion::new(version.get() + 1);

        let mut changes = self.changes.lock().unwrap();
        changes.push(CatalogChangeNotification {
            new_version: *version,
            previous_version: previous,
            invalidation_boundary_lsn: invalidation_lsn,
        });

        Ok(())
    }
}

impl Default for MockCatalogServer {
    fn default() -> Self {
        Self::new()
    }
}

impl CatalogServerTrait for MockCatalogServer {
    fn resolve_procedure(&self, procedure_id: ProcedureId) -> AndromedaResult<ProcedureManifest> {
        let procedures = self.procedures.lock().unwrap();
        procedures.get(&procedure_id).cloned().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                format!("procedure not found: {}", procedure_id.get()),
            )
        })
    }

    fn get_catalog_version(&self) -> CatalogVersion {
        *self.current_version.lock().unwrap()
    }

    fn subscribe_to_changes(&self) -> AndromedaResult<Box<dyn CatalogChangeSubscription>> {
        let changes = self.changes.lock().unwrap().clone();
        Ok(Box::new(MockCatalogChangeSubscription::new(changes)))
    }
}
