use andromeda_error::AndromedaResult;
use andromeda_types::{CatalogVersion, ProcedureId};

use super::{CatalogChangeSubscription, ProcedureManifest};

pub use andromeda_catalog_store::{
    CatalogManifestStoreBoundary, CatalogRuntimeEvidence, CatalogRuntimeReopenEvidence,
    CatalogRuntimeStore, CatalogServerRuntimeDiagnostic, CatalogServerRuntimeKind,
    DurableCatalogRuntimeHandle,
};

/// Catalog Server trait defining the boundary contract for catalog access.
///
/// All catalog queries by remote clients must go through this trait.
/// No ad hoc SQL is permitted; all queries are pre-authorized catalog views.
pub trait CatalogServerTrait: Send + Sync {
    /// Return the runtime durability diagnostic for this catalog server.
    fn runtime_diagnostic(&self) -> CatalogServerRuntimeDiagnostic;

    /// Resolve a procedure by ID to fetch its manifest.
    ///
    /// # Errors
    ///
    /// Returns `AndromedaErrorKind::Catalog` if:
    /// - The procedure ID does not exist
    /// - The procedure manifest cannot be loaded
    fn resolve_procedure(&self, procedure_id: ProcedureId) -> AndromedaResult<ProcedureManifest>;

    /// Get the current catalog version.
    ///
    /// This is a non-blocking, read-only query that returns the latest
    /// catalog version known by this server instance.
    fn get_catalog_version(&self) -> CatalogVersion;

    /// Subscribe to catalog version changes.
    ///
    /// The subscription will emit notifications whenever a procedure or
    /// other catalog object is created, modified, or deleted.
    ///
    /// # Errors
    ///
    /// Returns `AndromedaErrorKind::Resource` if:
    /// - The maximum number of concurrent subscriptions is exceeded
    /// - Memory allocation for the subscription fails
    fn subscribe_to_changes(&self) -> AndromedaResult<Box<dyn CatalogChangeSubscription>>;
}

/// Require a catalog server runtime that is backed by durable catalog semantics.
///
/// This boundary check prevents an in-memory mock from being passed off as a
/// production catalog runtime by callers that require durable publication and
/// recovery behavior.
pub fn require_durable_catalog_runtime(
    server: &dyn CatalogServerTrait,
) -> AndromedaResult<CatalogServerRuntimeDiagnostic> {
    let diagnostic = server.runtime_diagnostic();
    diagnostic.validate_for_durable_runtime()?;
    Ok(diagnostic)
}
