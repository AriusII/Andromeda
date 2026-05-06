use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use andromeda_core::{
    AbsencePolicy, AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion,
    ColumnDescriptor, ContractHash, DecimalType, FloatMode, FloatType, ProcedureId, ScalarType,
    TextEncoding, TimestampType, TypeDescriptor,
};
use andromeda_proto::generated::contract::v1::catalog_procedure_manifest_resolution_response::Status as ProtoCatalogManifestResolutionStatus;

use crate::{AccessMode, CatalogDefinition, CatalogSystemStore, ProcedureContract, QualifiedName};

use super::{
    CatalogChangeNotification, CatalogChangeSubscription, CatalogRuntimeEvidence,
    CatalogRuntimeReopenEvidence, CatalogRuntimeStore, CatalogServerRuntimeDiagnostic,
    CatalogServerTrait, CatalogSubscriptionRegistry, ColumnSchema, ProcedureManifest,
};

/// Catalog-owned manifest resolution status.
///
/// The generated Protobuf enum is accepted only at the boundary. This internal
/// status type intentionally has no `Unspecified` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogManifestResolutionStatus {
    Resolved,
    NotFound,
    CatalogVersionMismatch,
    ContractHashMismatch,
    NotSourceGeneratorReady,
    PermissionDenied,
    Unsupported,
    Malformed,
    Internal,
    CatalogNotReady,
    AuthRequired,
}

impl CatalogManifestResolutionStatus {
    pub const fn to_proto(self) -> ProtoCatalogManifestResolutionStatus {
        match self {
            Self::Resolved => ProtoCatalogManifestResolutionStatus::Resolved,
            Self::NotFound => ProtoCatalogManifestResolutionStatus::NotFound,
            Self::CatalogVersionMismatch => {
                ProtoCatalogManifestResolutionStatus::CatalogVersionMismatch
            }
            Self::ContractHashMismatch => {
                ProtoCatalogManifestResolutionStatus::ContractHashMismatch
            }
            Self::NotSourceGeneratorReady => {
                ProtoCatalogManifestResolutionStatus::NotSourceGeneratorReady
            }
            Self::PermissionDenied => ProtoCatalogManifestResolutionStatus::PermissionDenied,
            Self::Unsupported => ProtoCatalogManifestResolutionStatus::Unsupported,
            Self::Malformed => ProtoCatalogManifestResolutionStatus::Malformed,
            Self::Internal => ProtoCatalogManifestResolutionStatus::Internal,
            Self::CatalogNotReady => ProtoCatalogManifestResolutionStatus::CatalogNotReady,
            Self::AuthRequired => ProtoCatalogManifestResolutionStatus::AuthRequired,
        }
    }

    pub fn from_proto(status: ProtoCatalogManifestResolutionStatus) -> AndromedaResult<Self> {
        match status {
            ProtoCatalogManifestResolutionStatus::Resolved => Ok(Self::Resolved),
            ProtoCatalogManifestResolutionStatus::NotFound => Ok(Self::NotFound),
            ProtoCatalogManifestResolutionStatus::CatalogVersionMismatch => {
                Ok(Self::CatalogVersionMismatch)
            }
            ProtoCatalogManifestResolutionStatus::ContractHashMismatch => {
                Ok(Self::ContractHashMismatch)
            }
            ProtoCatalogManifestResolutionStatus::NotSourceGeneratorReady => {
                Ok(Self::NotSourceGeneratorReady)
            }
            ProtoCatalogManifestResolutionStatus::PermissionDenied => Ok(Self::PermissionDenied),
            ProtoCatalogManifestResolutionStatus::Unsupported => Ok(Self::Unsupported),
            ProtoCatalogManifestResolutionStatus::Malformed => Ok(Self::Malformed),
            ProtoCatalogManifestResolutionStatus::Internal => Ok(Self::Internal),
            ProtoCatalogManifestResolutionStatus::CatalogNotReady => Ok(Self::CatalogNotReady),
            ProtoCatalogManifestResolutionStatus::AuthRequired => Ok(Self::AuthRequired),
            ProtoCatalogManifestResolutionStatus::Unspecified => Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "catalog manifest resolution status must be specified",
            )),
        }
    }

    pub fn from_proto_i32(status: i32) -> AndromedaResult<Self> {
        match status {
            value if value == ProtoCatalogManifestResolutionStatus::Resolved as i32 => {
                Ok(Self::Resolved)
            }
            value if value == ProtoCatalogManifestResolutionStatus::NotFound as i32 => {
                Ok(Self::NotFound)
            }
            value
                if value == ProtoCatalogManifestResolutionStatus::CatalogVersionMismatch as i32 =>
            {
                Ok(Self::CatalogVersionMismatch)
            }
            value if value == ProtoCatalogManifestResolutionStatus::ContractHashMismatch as i32 => {
                Ok(Self::ContractHashMismatch)
            }
            value
                if value
                    == ProtoCatalogManifestResolutionStatus::NotSourceGeneratorReady as i32 =>
            {
                Ok(Self::NotSourceGeneratorReady)
            }
            value if value == ProtoCatalogManifestResolutionStatus::PermissionDenied as i32 => {
                Ok(Self::PermissionDenied)
            }
            value if value == ProtoCatalogManifestResolutionStatus::Unsupported as i32 => {
                Ok(Self::Unsupported)
            }
            value if value == ProtoCatalogManifestResolutionStatus::Malformed as i32 => {
                Ok(Self::Malformed)
            }
            value if value == ProtoCatalogManifestResolutionStatus::Internal as i32 => {
                Ok(Self::Internal)
            }
            value if value == ProtoCatalogManifestResolutionStatus::CatalogNotReady as i32 => {
                Ok(Self::CatalogNotReady)
            }
            value if value == ProtoCatalogManifestResolutionStatus::AuthRequired as i32 => {
                Ok(Self::AuthRequired)
            }
            value if value == ProtoCatalogManifestResolutionStatus::Unspecified as i32 => {
                Err(AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "catalog manifest resolution status must be specified",
                ))
            }
            _ => Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "unknown catalog manifest resolution status",
            )),
        }
    }

    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::Resolved => "STATUS_RESOLVED",
            Self::NotFound => "STATUS_NOT_FOUND",
            Self::CatalogVersionMismatch => "STATUS_CATALOG_VERSION_MISMATCH",
            Self::ContractHashMismatch => "STATUS_CONTRACT_HASH_MISMATCH",
            Self::NotSourceGeneratorReady => "STATUS_NOT_SOURCE_GENERATOR_READY",
            Self::PermissionDenied => "STATUS_PERMISSION_DENIED",
            Self::Unsupported => "STATUS_UNSUPPORTED",
            Self::Malformed => "STATUS_MALFORMED",
            Self::Internal => "STATUS_INTERNAL",
            Self::CatalogNotReady => "STATUS_CATALOG_NOT_READY",
            Self::AuthRequired => "STATUS_AUTH_REQUIRED",
        }
    }
}

fn catalog_runtime_lock_error(resource: &str) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Catalog,
        format!("catalog server runtime {resource} lock is poisoned"),
    )
}

/// Selector accepted by the catalog server manifest resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogManifestSelector {
    ProcedureId(ProcedureId),
    QualifiedName(QualifiedName),
}

impl CatalogManifestSelector {
    fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::ProcedureId(procedure_id) if procedure_id.get() == 0 => Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "catalog manifest selector procedure id must not be zero",
            )),
            Self::ProcedureId(_) | Self::QualifiedName(_) => Ok(()),
        }
    }
}

/// Runtime request for resolving a procedure manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolutionRequest {
    pub selector: CatalogManifestSelector,
    pub expected_contract_hash: Option<ContractHash>,
    pub expected_catalog_version: Option<CatalogVersion>,
    pub require_source_generator_ready: bool,
}

impl CatalogManifestResolutionRequest {
    pub fn by_id(procedure_id: ProcedureId) -> Self {
        Self {
            selector: CatalogManifestSelector::ProcedureId(procedure_id),
            expected_contract_hash: None,
            expected_catalog_version: None,
            require_source_generator_ready: false,
        }
    }

    pub fn by_qualified_name(name: QualifiedName) -> Self {
        Self {
            selector: CatalogManifestSelector::QualifiedName(name),
            expected_contract_hash: None,
            expected_catalog_version: None,
            require_source_generator_ready: false,
        }
    }

    pub fn by_name(name: &str) -> AndromedaResult<Self> {
        Ok(Self::by_qualified_name(QualifiedName::parse(name)?))
    }

    pub fn with_expected_contract_hash(mut self, contract_hash: ContractHash) -> Self {
        self.expected_contract_hash = Some(contract_hash);
        self
    }

    pub fn with_expected_catalog_version(mut self, catalog_version: CatalogVersion) -> Self {
        self.expected_catalog_version = Some(catalog_version);
        self
    }

    pub fn requiring_source_generator_ready(mut self) -> Self {
        self.require_source_generator_ready = true;
        self
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.selector.validate()?;

        if let Some(contract_hash) = self.expected_contract_hash
            && contract_hash.is_zero()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "catalog manifest expected contract hash must not be zero",
            ));
        }

        if let Some(catalog_version) = self.expected_catalog_version
            && catalog_version.get() == 0
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "catalog manifest expected catalog version must not be zero",
            ));
        }

        Ok(())
    }
}

/// Failure details returned by the runtime resolver before projection to the
/// governed proto catalog status model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogManifestResolutionFailure {
    NotFound,
    ContractHashMismatch {
        expected: ContractHash,
        actual: ContractHash,
    },
    CatalogVersionMismatch {
        expected: CatalogVersion,
        actual: CatalogVersion,
    },
    NotSourceGeneratorReady,
    PermissionDenied,
    CatalogNotReady,
    Malformed(String),
    Internal(String),
}

impl CatalogManifestResolutionFailure {
    pub fn status(&self) -> CatalogManifestResolutionStatus {
        match self {
            Self::NotFound => CatalogManifestResolutionStatus::NotFound,
            Self::ContractHashMismatch { .. } => {
                CatalogManifestResolutionStatus::ContractHashMismatch
            }
            Self::CatalogVersionMismatch { .. } => {
                CatalogManifestResolutionStatus::CatalogVersionMismatch
            }
            Self::NotSourceGeneratorReady => {
                CatalogManifestResolutionStatus::NotSourceGeneratorReady
            }
            Self::PermissionDenied => CatalogManifestResolutionStatus::PermissionDenied,
            Self::CatalogNotReady => CatalogManifestResolutionStatus::CatalogNotReady,
            Self::Malformed(_) => CatalogManifestResolutionStatus::Malformed,
            Self::Internal(_) => CatalogManifestResolutionStatus::Internal,
        }
    }

    pub fn diagnostic_code(&self) -> &'static str {
        self.status().diagnostic_code()
    }
}

/// Resolver outcome with proto catalog status mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolution {
    pub status: CatalogManifestResolutionStatus,
    pub manifest: Option<ProcedureManifest>,
    pub current_catalog_version: CatalogVersion,
    pub diagnostic_code: Option<&'static str>,
    pub failure: Option<CatalogManifestResolutionFailure>,
}

impl CatalogManifestResolution {
    fn resolved(manifest: ProcedureManifest, current_catalog_version: CatalogVersion) -> Self {
        Self {
            status: CatalogManifestResolutionStatus::Resolved,
            manifest: Some(manifest),
            current_catalog_version,
            diagnostic_code: None,
            failure: None,
        }
    }

    fn failed(
        failure: CatalogManifestResolutionFailure,
        current_catalog_version: CatalogVersion,
    ) -> Self {
        Self {
            status: failure.status(),
            manifest: None,
            current_catalog_version,
            diagnostic_code: Some(failure.diagnostic_code()),
            failure: Some(failure),
        }
    }

    pub fn is_resolved(&self) -> bool {
        self.status == CatalogManifestResolutionStatus::Resolved
    }
}

/// Runtime metadata attached to a resolved manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogManifestRuntimeMetadata {
    pub source_generator_ready: bool,
    pub permission_granted: bool,
}

impl CatalogManifestRuntimeMetadata {
    pub const fn ready() -> Self {
        Self {
            source_generator_ready: true,
            permission_granted: true,
        }
    }
}

impl Default for CatalogManifestRuntimeMetadata {
    fn default() -> Self {
        Self::ready()
    }
}

/// Manifest plus runtime gates read from a catalog-backed store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestRecord {
    pub manifest: ProcedureManifest,
    pub metadata: CatalogManifestRuntimeMetadata,
}

impl CatalogManifestRecord {
    pub fn new(
        manifest: ProcedureManifest,
        metadata: CatalogManifestRuntimeMetadata,
    ) -> AndromedaResult<Self> {
        manifest.validate()?;
        Ok(Self { manifest, metadata })
    }
}

/// Store boundary consumed by `CatalogServerRuntime`.
pub trait CatalogManifestStore: CatalogRuntimeStore {
    fn current_catalog_version(&self) -> CatalogVersion;

    fn resolve_manifest_by_id(
        &self,
        procedure_id: ProcedureId,
    ) -> AndromedaResult<Option<CatalogManifestRecord>>;

    fn resolve_manifest_by_name(
        &self,
        name: &QualifiedName,
    ) -> AndromedaResult<Option<CatalogManifestRecord>>;
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
        Self {
            store: Arc::new(RwLock::new(store)),
            runtime_metadata: Arc::new(RwLock::new(BTreeMap::new())),
            reopen_evidence: None,
        }
    }

    pub fn with_reopen_evidence(
        store: CatalogSystemStore,
        reopen_evidence: CatalogRuntimeReopenEvidence,
    ) -> Self {
        Self {
            store: Arc::new(RwLock::new(store)),
            runtime_metadata: Arc::new(RwLock::new(BTreeMap::new())),
            reopen_evidence: Some(reopen_evidence),
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
}

impl CatalogRuntimeStore for CatalogSnapshotManifestStore {
    fn catalog_runtime_evidence(&self) -> CatalogRuntimeEvidence {
        let store = match self.store.read() {
            Ok(store) => store,
            Err(poisoned) => poisoned.into_inner(),
        };
        let catalog_version = Some(store.snapshot().visible_version());
        match self.reopen_evidence {
            Some(reopen_evidence) => {
                CatalogRuntimeEvidence::durable(catalog_version, None, reopen_evidence)
            }
            None => CatalogRuntimeEvidence::durable_without_reopen_evidence(catalog_version, None),
        }
    }
}

impl CatalogManifestStore for CatalogSnapshotManifestStore {
    fn current_catalog_version(&self) -> CatalogVersion {
        let store = match self.store.read() {
            Ok(store) => store,
            Err(poisoned) => poisoned.into_inner(),
        };
        store.snapshot().visible_version()
    }

    fn resolve_manifest_by_id(
        &self,
        procedure_id: ProcedureId,
    ) -> AndromedaResult<Option<CatalogManifestRecord>> {
        let store = self
            .store
            .read()
            .map_err(|_| catalog_runtime_lock_error("system store"))?;
        let Some(contract) = store.snapshot().get_procedure_by_id(procedure_id) else {
            return Ok(None);
        };
        if contract.object.catalog_version > store.snapshot().visible_version()
            || !store.snapshot().is_active_object(contract.object.object_id)
        {
            return Ok(None);
        }
        self.record_from_contract(contract).map(Some)
    }

    fn resolve_manifest_by_name(
        &self,
        name: &QualifiedName,
    ) -> AndromedaResult<Option<CatalogManifestRecord>> {
        let store = self
            .store
            .read()
            .map_err(|_| catalog_runtime_lock_error("system store"))?;
        let Some(CatalogDefinition::Procedure(contract)) = store.snapshot().get_by_name(name)
        else {
            return Ok(None);
        };
        if contract.object.catalog_version > store.snapshot().visible_version()
            || !store.snapshot().is_active_object(contract.object.object_id)
        {
            return Ok(None);
        }
        self.record_from_contract(contract).map(Some)
    }
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
            Err(error) => CatalogManifestResolution::failed(
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
            return CatalogManifestResolution::failed(
                CatalogManifestResolutionFailure::Malformed(error.message().to_string()),
                current_catalog_version,
            );
        }

        let ready = match self.readiness.read() {
            Ok(ready) => *ready,
            Err(_) => {
                return CatalogManifestResolution::failed(
                    CatalogManifestResolutionFailure::Internal(
                        "catalog server runtime readiness lock is poisoned".to_string(),
                    ),
                    current_catalog_version,
                );
            }
        };
        if !ready {
            return CatalogManifestResolution::failed(
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
                return CatalogManifestResolution::failed(
                    CatalogManifestResolutionFailure::Internal(error.message().to_string()),
                    current_catalog_version,
                );
            }
        }) else {
            return CatalogManifestResolution::failed(
                CatalogManifestResolutionFailure::NotFound,
                current_catalog_version,
            );
        };

        if let Some(expected) = request.expected_contract_hash {
            let actual = match ContractHash::from_slice(&record.manifest.contract_hash) {
                Ok(actual) => actual,
                Err(error) => {
                    return CatalogManifestResolution::failed(
                        CatalogManifestResolutionFailure::Internal(error.message().to_string()),
                        current_catalog_version,
                    );
                }
            };
            if actual != expected {
                return CatalogManifestResolution::failed(
                    CatalogManifestResolutionFailure::ContractHashMismatch { expected, actual },
                    current_catalog_version,
                );
            }
        }

        if let Some(expected) = request.expected_catalog_version {
            let actual = record.manifest.catalog_version;
            if actual != expected {
                return CatalogManifestResolution::failed(
                    CatalogManifestResolutionFailure::CatalogVersionMismatch { expected, actual },
                    current_catalog_version,
                );
            }
        }

        if !record.metadata.permission_granted {
            return CatalogManifestResolution::failed(
                CatalogManifestResolutionFailure::PermissionDenied,
                current_catalog_version,
            );
        }

        if request.require_source_generator_ready && !record.metadata.source_generator_ready {
            return CatalogManifestResolution::failed(
                CatalogManifestResolutionFailure::NotSourceGeneratorReady,
                current_catalog_version,
            );
        }

        CatalogManifestResolution::resolved(record.manifest, current_catalog_version)
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

fn andromeda_error_from_resolution(resolution: &CatalogManifestResolution) -> AndromedaError {
    let kind = match resolution.status {
        CatalogManifestResolutionStatus::ContractHashMismatch
        | CatalogManifestResolutionStatus::Malformed => AndromedaErrorKind::Contract,
        CatalogManifestResolutionStatus::PermissionDenied
        | CatalogManifestResolutionStatus::AuthRequired => AndromedaErrorKind::Security,
        CatalogManifestResolutionStatus::Internal => AndromedaErrorKind::Internal,
        _ => AndromedaErrorKind::Catalog,
    };
    let diagnostic = resolution
        .diagnostic_code
        .unwrap_or_else(|| resolution.status.diagnostic_code());
    AndromedaError::new(
        kind,
        format!("catalog manifest resolution failed: {diagnostic}"),
    )
}

fn manifest_from_contract(contract: &ProcedureContract) -> AndromedaResult<ProcedureManifest> {
    contract.validate_canonical_hash()?;
    let output_schema = contract
        .result_streams
        .first()
        .map(|stream| columns_to_schema(&stream.columns))
        .transpose()?
        .unwrap_or_default();
    let manifest = ProcedureManifest {
        procedure_id: contract.procedure_id,
        qualified_name: contract.object.name.as_catalog_path(),
        catalog_version: contract.object.catalog_version,
        contract_hash: contract.contract_hash.as_bytes().to_vec(),
        input_schema: columns_to_schema(&contract.inputs)?,
        output_schema,
        is_mutable: matches!(
            contract.transaction_policy.access_mode,
            AccessMode::ReadWrite
        ),
        min_compatible_version: CatalogVersion::new(1),
    };
    manifest.validate()?;
    Ok(manifest)
}

fn columns_to_schema(columns: &[ColumnDescriptor]) -> AndromedaResult<Vec<ColumnSchema>> {
    columns
        .iter()
        .map(|column| {
            column.validate()?;
            Ok(ColumnSchema {
                name: column.name.clone(),
                type_descriptor: type_descriptor_name(&column.data_type),
                ordinal: column.ordinal,
                nullable: matches!(column.data_type.absence, AbsencePolicy::ExplicitOptional),
            })
        })
        .collect()
}

fn type_descriptor_name(descriptor: &TypeDescriptor) -> String {
    match &descriptor.scalar {
        ScalarType::I8 => "int8".to_string(),
        ScalarType::I16 => "int16".to_string(),
        ScalarType::I32 => "int32".to_string(),
        ScalarType::I64 => "int64".to_string(),
        ScalarType::I128 => "int128".to_string(),
        ScalarType::U8 => "uint8".to_string(),
        ScalarType::U16 => "uint16".to_string(),
        ScalarType::U32 => "uint32".to_string(),
        ScalarType::U64 => "uint64".to_string(),
        ScalarType::U128 => "uint128".to_string(),
        ScalarType::Decimal(decimal) => decimal_type_name(*decimal),
        ScalarType::Float(float) => float_type_name(*float),
        ScalarType::Bool => "bool".to_string(),
        ScalarType::Text(text) => text_type_name(text.encoding),
        ScalarType::Timestamp(timestamp) => timestamp_type_name(*timestamp),
    }
}

fn decimal_type_name(decimal: DecimalType) -> String {
    match decimal {
        DecimalType::Min => "decimal(min)".to_string(),
        DecimalType::Mid => "decimal(mid)".to_string(),
        DecimalType::Max => "decimal(max)".to_string(),
        DecimalType::Custom { precision, scale } => format!("decimal({precision},{scale})"),
    }
}

fn float_type_name(float: FloatType) -> String {
    match float {
        FloatType::Min => "float(min)".to_string(),
        FloatType::Mid => "float(mid)".to_string(),
        FloatType::Max => "float(max)".to_string(),
        FloatType::Custom { bits, mode } => {
            let mode = match mode {
                FloatMode::Approximate => "approx",
                FloatMode::DeterministicAnalytics => "deterministic",
            };
            format!("float({bits},{mode})")
        }
    }
}

fn text_type_name(encoding: TextEncoding) -> String {
    match encoding {
        TextEncoding::Utf8 => "text(utf8)".to_string(),
        TextEncoding::Utf16 => "text(utf16)".to_string(),
        TextEncoding::Unicode => "text(unicode)".to_string(),
    }
}

fn timestamp_type_name(timestamp: TimestampType) -> String {
    match timestamp {
        TimestampType::Transaction => "timestamp(transaction)".to_string(),
        TimestampType::Invocation => "timestamp(invocation)".to_string(),
        TimestampType::MonotonicEpoch => "timestamp(monotonic_epoch)".to_string(),
    }
}
