use std::sync::{Arc, RwLock};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use crate::{CatalogSystemStore, QualifiedName};

use super::{
    CatalogChangeNotification, CatalogChangeSubscription, CatalogRuntimeEvidence,
    CatalogRuntimeStore, CatalogServerRuntimeDiagnostic, CatalogServerTrait,
    CatalogSubscriptionRegistry, ProcedureManifest,
};

mod record;
mod request;
mod resolution;
mod schema;
mod status;
mod store;

use self::resolution::{
    andromeda_error_from_resolution, failed as failed_manifest_resolution,
    resolved as resolved_manifest_resolution,
};
pub use self::{
    record::{CatalogManifestRecord, CatalogManifestRuntimeMetadata},
    request::{CatalogManifestResolutionRequest, CatalogManifestSelector},
    resolution::{CatalogManifestResolution, CatalogManifestResolutionFailure},
    status::CatalogManifestResolutionStatus,
    store::{CatalogManifestStore, CatalogSnapshotManifestStore},
};

fn catalog_runtime_lock_error(resource: &str) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Catalog,
        format!("catalog server runtime {resource} lock is poisoned"),
    )
}

/// Catalog server runtime backed by a real manifest store and subscription
/// registry.
#[derive(Clone)]
pub struct CatalogServerRuntime {
    store: Arc<dyn CatalogManifestStore>,
    subscription_registry: CatalogSubscriptionRegistry,
    readiness: Arc<RwLock<bool>>,
}

impl CatalogServerRuntime {
    pub fn new(store: Arc<dyn CatalogManifestStore>) -> Self {
        Self {
            store,
            subscription_registry: CatalogSubscriptionRegistry::new(),
            readiness: Arc::new(RwLock::new(true)),
        }
    }

    pub fn with_subscription_registry(
        store: Arc<dyn CatalogManifestStore>,
        subscription_registry: CatalogSubscriptionRegistry,
    ) -> Self {
        Self {
            store,
            subscription_registry,
            readiness: Arc::new(RwLock::new(true)),
        }
    }

    pub fn from_system_store(store: CatalogSystemStore) -> Self {
        Self::new(Arc::new(CatalogSnapshotManifestStore::new(store)))
    }

    pub fn set_ready(&self, ready: bool) -> AndromedaResult<()> {
        let mut readiness = self
            .readiness
            .write()
            .map_err(|_| catalog_runtime_lock_error("readiness"))?;
        *readiness = ready;
        Ok(())
    }

    pub fn publish_change(&self, change: CatalogChangeNotification) -> AndromedaResult<()> {
        self.subscription_registry.publish(change)
    }

    pub fn resolve_manifest_by_id(&self, procedure_id: ProcedureId) -> CatalogManifestResolution {
        self.resolve_manifest(CatalogManifestResolutionRequest::by_id(procedure_id))
    }

    pub fn resolve_manifest_by_qualified_name(
        &self,
        name: QualifiedName,
    ) -> CatalogManifestResolution {
        self.resolve_manifest(CatalogManifestResolutionRequest::by_qualified_name(name))
    }

    pub fn resolve_manifest_by_name(&self, name: &str) -> CatalogManifestResolution {
        match CatalogManifestResolutionRequest::by_name(name) {
            Ok(request) => self.resolve_manifest(request),
            Err(error) => failed_manifest_resolution(
                CatalogManifestResolutionFailure::Malformed(error.message().to_string()),
                self.store.current_catalog_version(),
            ),
        }
    }

    pub fn resolve_manifest(
        &self,
        request: CatalogManifestResolutionRequest,
    ) -> CatalogManifestResolution {
        let current_catalog_version = self.store.current_catalog_version();

        if let Err(error) = request.validate() {
            return failed_manifest_resolution(
                CatalogManifestResolutionFailure::Malformed(error.message().to_string()),
                current_catalog_version,
            );
        }

        let ready = match self.readiness.read() {
            Ok(ready) => *ready,
            Err(_) => {
                return failed_manifest_resolution(
                    CatalogManifestResolutionFailure::Internal(
                        "catalog server runtime readiness lock is poisoned".to_string(),
                    ),
                    current_catalog_version,
                );
            }
        };
        if !ready {
            return failed_manifest_resolution(
                CatalogManifestResolutionFailure::CatalogNotReady,
                current_catalog_version,
            );
        }

        let record = match &request.selector {
            CatalogManifestSelector::ProcedureId(procedure_id) => {
                self.store.resolve_manifest_by_id(*procedure_id)
            }
            CatalogManifestSelector::QualifiedName(name) => {
                self.store.resolve_manifest_by_name(name)
            }
        };

        let Some(record) = (match record {
            Ok(record) => record,
            Err(error) => {
                return failed_manifest_resolution(
                    CatalogManifestResolutionFailure::Internal(error.message().to_string()),
                    current_catalog_version,
                );
            }
        }) else {
            return failed_manifest_resolution(
                CatalogManifestResolutionFailure::NotFound,
                current_catalog_version,
            );
        };

        if let Some(expected) = request.expected_contract_hash {
            let actual = match ContractHash::from_slice(&record.manifest.contract_hash) {
                Ok(actual) => actual,
                Err(error) => {
                    return failed_manifest_resolution(
                        CatalogManifestResolutionFailure::Internal(error.message().to_string()),
                        current_catalog_version,
                    );
                }
            };
            if actual != expected {
                return failed_manifest_resolution(
                    CatalogManifestResolutionFailure::ContractHashMismatch { expected, actual },
                    current_catalog_version,
                );
            }
        }

        if let Some(expected) = request.expected_catalog_version {
            let actual = record.manifest.catalog_version;
            if actual != expected {
                return failed_manifest_resolution(
                    CatalogManifestResolutionFailure::CatalogVersionMismatch { expected, actual },
                    current_catalog_version,
                );
            }
        }

        if !record.metadata.permission_granted {
            return failed_manifest_resolution(
                CatalogManifestResolutionFailure::PermissionDenied,
                current_catalog_version,
            );
        }

        if request.require_source_generator_ready && !record.metadata.source_generator_ready {
            return failed_manifest_resolution(
                CatalogManifestResolutionFailure::NotSourceGeneratorReady,
                current_catalog_version,
            );
        }

        resolved_manifest_resolution(record.manifest, current_catalog_version)
    }
}

impl CatalogRuntimeStore for CatalogServerRuntime {
    fn catalog_runtime_evidence(&self) -> CatalogRuntimeEvidence {
        self.store.catalog_runtime_evidence()
    }
}

impl CatalogServerTrait for CatalogServerRuntime {
    fn runtime_diagnostic(&self) -> CatalogServerRuntimeDiagnostic {
        self.catalog_runtime_evidence().diagnostic()
    }

    fn resolve_procedure(&self, procedure_id: ProcedureId) -> AndromedaResult<ProcedureManifest> {
        let resolution = self.resolve_manifest_by_id(procedure_id);
        if resolution.status == CatalogManifestResolutionStatus::Resolved {
            resolution.manifest.ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Internal,
                    "catalog manifest resolution returned resolved status without manifest",
                )
            })
        } else {
            Err(andromeda_error_from_resolution(&resolution))
        }
    }

    fn get_catalog_version(&self) -> CatalogVersion {
        self.store.current_catalog_version()
    }

    fn subscribe_to_changes(&self) -> AndromedaResult<Box<dyn CatalogChangeSubscription>> {
        Ok(Box::new(self.subscription_registry.subscribe()?))
    }
}
