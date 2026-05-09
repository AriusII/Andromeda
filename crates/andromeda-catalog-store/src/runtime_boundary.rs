//! Runtime-free catalog runtime/store boundary traits.
//!
//! These types capture durable runtime evidence and the minimal store-facing
//! traits required by catalog server runtimes, without owning runtime
//! execution, publication flows, recovery orchestration, or storage engines.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ProcedureId};

use crate::QualifiedName;

/// Declares the durability boundary implemented by a catalog server runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogServerRuntimeKind {
    /// In-memory mock runtime; suitable for tests and local contract exercises only.
    MockEphemeral,
    /// Runtime backed by durable catalog publication and recovery semantics.
    Durable,
}

/// Evidence that the catalog runtime has been reopened from its durable source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogRuntimeReopenEvidence {
    /// Monotonic durable marker observed after reopening the catalog runtime.
    pub marker: u64,
    /// Short source label for the reopened evidence, for example a WAL or
    /// file-backed catalog store identifier.
    pub source: &'static str,
}

impl CatalogRuntimeReopenEvidence {
    pub const fn new(marker: u64, source: &'static str) -> Self {
        Self { marker, source }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.marker == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog runtime reopen evidence marker must not be zero",
            ));
        }

        if self.source.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog runtime reopen evidence source must not be empty",
            ));
        }

        Ok(())
    }
}

/// Durable-readiness evidence exposed by a catalog runtime store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogRuntimeEvidence {
    pub kind: CatalogServerRuntimeKind,
    pub catalog_version: Option<CatalogVersion>,
    pub epoch: Option<u64>,
    pub reopen_evidence: Option<CatalogRuntimeReopenEvidence>,
}

impl CatalogRuntimeEvidence {
    pub const fn mock_ephemeral() -> Self {
        Self {
            kind: CatalogServerRuntimeKind::MockEphemeral,
            catalog_version: None,
            epoch: None,
            reopen_evidence: None,
        }
    }

    pub const fn durable(
        catalog_version: Option<CatalogVersion>,
        epoch: Option<u64>,
        reopen_evidence: CatalogRuntimeReopenEvidence,
    ) -> Self {
        Self {
            kind: CatalogServerRuntimeKind::Durable,
            catalog_version,
            epoch,
            reopen_evidence: Some(reopen_evidence),
        }
    }

    pub const fn durable_without_reopen_evidence(
        catalog_version: Option<CatalogVersion>,
        epoch: Option<u64>,
    ) -> Self {
        Self {
            kind: CatalogServerRuntimeKind::Durable,
            catalog_version,
            epoch,
            reopen_evidence: None,
        }
    }

    pub fn validate_for_durable_runtime(self) -> AndromedaResult<()> {
        if !matches!(self.kind, CatalogServerRuntimeKind::Durable) {
            let diagnostic = self.diagnostic();
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                format!("{}; {}", diagnostic.reason, diagnostic.actionable_message),
            ));
        }

        if let Some(version) = self.catalog_version
            && version.get() == 0
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "durable catalog runtime evidence catalog version must not be zero",
            ));
        }

        let Some(reopen_evidence) = self.reopen_evidence else {
            let diagnostic = self.diagnostic();
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                format!("{}; {}", diagnostic.reason, diagnostic.actionable_message),
            ));
        };

        reopen_evidence.validate()
    }

    pub fn diagnostic(self) -> CatalogServerRuntimeDiagnostic {
        let (reason, actionable_message) = match self.kind {
            CatalogServerRuntimeKind::MockEphemeral => (
                "catalog runtime is MockEphemeral and keeps catalog state in memory only",
                "use a catalog runtime store that can reopen committed catalog evidence before enabling durable runtime mode",
            ),
            CatalogServerRuntimeKind::Durable if self.reopen_evidence.is_none() => (
                "catalog runtime is marked Durable but did not provide reopen evidence",
                "attach validated reopen evidence from the durable catalog store before accepting the runtime as durable",
            ),
            CatalogServerRuntimeKind::Durable
                if self
                    .reopen_evidence
                    .is_some_and(|evidence| evidence.validate().is_err()) =>
            {
                (
                    "catalog runtime provided invalid durable reopen evidence",
                    "provide a non-zero reopen marker and non-empty source before accepting the runtime as durable",
                )
            },
            CatalogServerRuntimeKind::Durable => (
                "catalog runtime has validated durable reopen evidence",
                "no catalog runtime durability action required",
            ),
        };

        CatalogServerRuntimeDiagnostic {
            kind: self.kind,
            catalog_version: self.catalog_version,
            epoch: self.epoch,
            reopen_evidence: self.reopen_evidence,
            reason,
            actionable_message,
        }
    }
}

/// Runtime diagnostic exposed by catalog server implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogServerRuntimeDiagnostic {
    pub kind: CatalogServerRuntimeKind,
    pub catalog_version: Option<CatalogVersion>,
    pub epoch: Option<u64>,
    pub reopen_evidence: Option<CatalogRuntimeReopenEvidence>,
    pub reason: &'static str,
    pub actionable_message: &'static str,
}

impl CatalogServerRuntimeDiagnostic {
    pub const fn mock_ephemeral() -> Self {
        Self {
            kind: CatalogServerRuntimeKind::MockEphemeral,
            catalog_version: None,
            epoch: None,
            reopen_evidence: None,
            reason: "catalog runtime is MockEphemeral and keeps catalog state in memory only",
            actionable_message: "use a catalog runtime store that can reopen committed catalog evidence before enabling durable runtime mode",
        }
    }

    pub const fn durable() -> Self {
        Self {
            kind: CatalogServerRuntimeKind::Durable,
            catalog_version: None,
            epoch: None,
            reopen_evidence: None,
            reason: "catalog runtime is marked Durable but did not provide reopen evidence",
            actionable_message: "attach validated reopen evidence from the durable catalog store before accepting the runtime as durable",
        }
    }

    pub fn durable_with_reopen_evidence(
        catalog_version: Option<CatalogVersion>,
        epoch: Option<u64>,
        reopen_evidence: CatalogRuntimeReopenEvidence,
    ) -> Self {
        CatalogRuntimeEvidence {
            kind: CatalogServerRuntimeKind::Durable,
            catalog_version,
            epoch,
            reopen_evidence: Some(reopen_evidence),
        }
        .diagnostic()
    }

    pub fn evidence(self) -> CatalogRuntimeEvidence {
        CatalogRuntimeEvidence {
            kind: self.kind,
            catalog_version: self.catalog_version,
            epoch: self.epoch,
            reopen_evidence: self.reopen_evidence,
        }
    }

    pub fn validate_for_durable_runtime(self) -> AndromedaResult<()> {
        self.evidence().validate_for_durable_runtime()
    }

    pub fn is_durable(self) -> bool {
        self.validate_for_durable_runtime().is_ok()
    }
}

/// Catalog runtime store capability used to validate durable runtime readiness.
pub trait CatalogRuntimeStore: Send + Sync {
    fn catalog_runtime_evidence(&self) -> CatalogRuntimeEvidence;
}

/// A validated handle proving the catalog runtime met the durable boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableCatalogRuntimeHandle {
    evidence: CatalogRuntimeEvidence,
}

impl DurableCatalogRuntimeHandle {
    pub fn validate(store: &dyn CatalogRuntimeStore) -> AndromedaResult<Self> {
        Self::from_evidence(store.catalog_runtime_evidence())
    }

    pub fn from_evidence(evidence: CatalogRuntimeEvidence) -> AndromedaResult<Self> {
        evidence.validate_for_durable_runtime()?;
        Ok(Self { evidence })
    }

    pub const fn evidence(self) -> CatalogRuntimeEvidence {
        self.evidence
    }
}

/// Store boundary consumed by catalog server runtimes for manifest resolution.
pub trait CatalogManifestStoreBoundary<Record>: CatalogRuntimeStore {
    fn current_catalog_version(&self) -> CatalogVersion;

    fn resolve_manifest_by_id(&self, procedure_id: ProcedureId) -> AndromedaResult<Option<Record>>;

    fn resolve_manifest_by_name(&self, name: &QualifiedName) -> AndromedaResult<Option<Record>>;
}
