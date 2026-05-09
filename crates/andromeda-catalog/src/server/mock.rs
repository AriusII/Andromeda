use std::sync::Arc;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ProcedureId};

use super::{
    CatalogChangeNotification, CatalogChangeSubscription, CatalogRuntimeEvidence,
    CatalogRuntimeStore, CatalogServerRuntimeDiagnostic, CatalogServerTrait,
    MockCatalogChangeSubscription, ProcedureManifest,
};

fn catalog_mock_lock_error(resource: &str) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Catalog,
        format!("mock catalog server {resource} lock is poisoned"),
    )
}

/// Mock implementation of `CatalogServerTrait` for testing.
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
        let mut procedures = self
            .procedures
            .lock()
            .map_err(|_| catalog_mock_lock_error("procedures"))?;
        procedures.insert(manifest.procedure_id, manifest);
        Ok(())
    }

    /// Advance the catalog version and emit a change notification.
    pub fn advance_catalog_version(&self, invalidation_lsn: u64) -> AndromedaResult<()> {
        let mut version = self
            .current_version
            .lock()
            .map_err(|_| catalog_mock_lock_error("current version"))?;
        let mut changes = self
            .changes
            .lock()
            .map_err(|_| catalog_mock_lock_error("changes"))?;
        let previous = *version;
        *version = CatalogVersion::new(version.get() + 1);

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

impl CatalogRuntimeStore for MockCatalogServer {
    fn catalog_runtime_evidence(&self) -> CatalogRuntimeEvidence {
        CatalogRuntimeEvidence::mock_ephemeral()
    }
}

impl CatalogServerTrait for MockCatalogServer {
    fn runtime_diagnostic(&self) -> CatalogServerRuntimeDiagnostic {
        self.catalog_runtime_evidence().diagnostic()
    }

    fn resolve_procedure(&self, procedure_id: ProcedureId) -> AndromedaResult<ProcedureManifest> {
        let procedures = self
            .procedures
            .lock()
            .map_err(|_| catalog_mock_lock_error("procedures"))?;
        procedures.get(&procedure_id).cloned().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                format!("procedure not found: {}", procedure_id.get()),
            )
        })
    }

    fn get_catalog_version(&self) -> CatalogVersion {
        match self.current_version.lock() {
            Ok(version) => *version,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    fn subscribe_to_changes(&self) -> AndromedaResult<Box<dyn CatalogChangeSubscription>> {
        let changes = self
            .changes
            .lock()
            .map_err(|_| catalog_mock_lock_error("changes"))?
            .clone();
        Ok(Box::new(MockCatalogChangeSubscription::new(changes)))
    }
}
