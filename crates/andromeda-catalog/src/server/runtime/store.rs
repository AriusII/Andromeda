use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use andromeda_catalog_store::{
    CatalogDefinition, CatalogManifestRecord, CatalogManifestRuntimeMetadata,
    CatalogManifestStoreBoundary, QualifiedName,
};
use andromeda_error::AndromedaResult;
use andromeda_procedure_contract::ProcedureContract;
use andromeda_types::{CatalogVersion, ProcedureId};

use crate::CatalogSystemStore;

use super::super::{CatalogRuntimeEvidence, CatalogRuntimeReopenEvidence, CatalogRuntimeStore};
use super::{catalog_runtime_lock_error, schema::manifest_from_contract};

/// Store boundary consumed by `CatalogServerRuntime`.
pub trait CatalogManifestStore: CatalogManifestStoreBoundary<CatalogManifestRecord> {}

impl<T> CatalogManifestStore for T where
    T: CatalogManifestStoreBoundary<CatalogManifestRecord> + ?Sized
{
}

/// Read adapter over the real catalog system store.
#[derive(Debug, Clone)]
pub struct CatalogSnapshotManifestStore {
    store: Arc<RwLock<CatalogSystemStore>>,
    runtime_metadata: Arc<RwLock<BTreeMap<ProcedureId, CatalogManifestRuntimeMetadata>>>,
    reopen_evidence: Option<CatalogRuntimeReopenEvidence>,
}

impl CatalogSnapshotManifestStore {
    pub fn new(store: CatalogSystemStore) -> Self {
        Self::from_parts(store, None)
    }

    pub fn with_reopen_evidence(
        store: CatalogSystemStore,
        reopen_evidence: CatalogRuntimeReopenEvidence,
    ) -> Self {
        Self::from_parts(store, Some(reopen_evidence))
    }

    fn from_parts(
        store: CatalogSystemStore,
        reopen_evidence: Option<CatalogRuntimeReopenEvidence>,
    ) -> Self {
        Self {
            store: Arc::new(RwLock::new(store)),
            runtime_metadata: Arc::new(RwLock::new(BTreeMap::new())),
            reopen_evidence,
        }
    }

    pub fn shared_store(&self) -> Arc<RwLock<CatalogSystemStore>> {
        Arc::clone(&self.store)
    }

    pub fn set_source_generator_ready(
        &self,
        procedure_id: ProcedureId,
        ready: bool,
    ) -> AndromedaResult<()> {
        let mut metadata = self
            .runtime_metadata
            .write()
            .map_err(|_| catalog_runtime_lock_error("metadata"))?;
        metadata
            .entry(procedure_id)
            .or_default()
            .source_generator_ready = ready;
        Ok(())
    }

    pub fn set_permission_granted(
        &self,
        procedure_id: ProcedureId,
        granted: bool,
    ) -> AndromedaResult<()> {
        let mut metadata = self
            .runtime_metadata
            .write()
            .map_err(|_| catalog_runtime_lock_error("metadata"))?;
        metadata.entry(procedure_id).or_default().permission_granted = granted;
        Ok(())
    }

    fn read_store(&self) -> AndromedaResult<RwLockReadGuard<'_, CatalogSystemStore>> {
        self.store
            .read()
            .map_err(|_| catalog_runtime_lock_error("system store"))
    }

    fn metadata_for(
        &self,
        procedure_id: ProcedureId,
    ) -> AndromedaResult<CatalogManifestRuntimeMetadata> {
        let metadata = self
            .runtime_metadata
            .read()
            .map_err(|_| catalog_runtime_lock_error("metadata"))?;
        Ok(metadata.get(&procedure_id).copied().unwrap_or_default())
    }

    fn record_from_contract(
        &self,
        contract: &ProcedureContract,
    ) -> AndromedaResult<CatalogManifestRecord> {
        let manifest = manifest_from_contract(contract)?;
        CatalogManifestRecord::new(manifest, self.metadata_for(contract.procedure_id)?)
    }

    fn record_from_visible_contract(
        &self,
        store: &CatalogSystemStore,
        contract: &ProcedureContract,
    ) -> AndromedaResult<Option<CatalogManifestRecord>> {
        if !contract_is_visible_and_active(store, contract) {
            return Ok(None);
        }
        self.record_from_contract(contract).map(Some)
    }
}

impl CatalogRuntimeStore for CatalogSnapshotManifestStore {
    fn catalog_runtime_evidence(&self) -> CatalogRuntimeEvidence {
        let store = self
            .store
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let catalog_version = Some(store.snapshot().visible_version());
        match self.reopen_evidence {
            Some(reopen_evidence) => {
                CatalogRuntimeEvidence::durable(catalog_version, None, reopen_evidence)
            },
            None => CatalogRuntimeEvidence::durable_without_reopen_evidence(catalog_version, None),
        }
    }
}

impl CatalogManifestStoreBoundary<CatalogManifestRecord> for CatalogSnapshotManifestStore {
    fn current_catalog_version(&self) -> CatalogVersion {
        let store = self
            .store
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        store.snapshot().visible_version()
    }

    fn resolve_manifest_by_id(
        &self,
        procedure_id: ProcedureId,
    ) -> AndromedaResult<Option<CatalogManifestRecord>> {
        let store = self.read_store()?;
        let Some(contract) = store.snapshot().visible_get_procedure_by_id(procedure_id) else {
            return Ok(None);
        };
        self.record_from_visible_contract(&store, contract)
    }

    fn resolve_manifest_by_name(
        &self,
        name: &QualifiedName,
    ) -> AndromedaResult<Option<CatalogManifestRecord>> {
        let store = self.read_store()?;
        let Some(CatalogDefinition::Procedure(contract)) =
            store.snapshot().visible_get_by_name(name)
        else {
            return Ok(None);
        };
        self.record_from_visible_contract(&store, contract)
    }
}

fn contract_is_visible_and_active(
    store: &CatalogSystemStore,
    contract: &ProcedureContract,
) -> bool {
    let snapshot = store.snapshot();
    contract.object.catalog_version <= snapshot.visible_version()
        && snapshot.is_active_object(contract.object.object_id)
}
