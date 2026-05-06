use andromeda_core::{AndromedaResult, CatalogVersion, ProcedureId};

use super::{CatalogChangeSubscription, ProcedureManifest};

/// Catalog Server trait defining the boundary contract for catalog access.
///
/// All catalog queries by remote clients must go through this trait.
/// No ad hoc SQL is permitted; all queries are pre-authorized catalog views.
pub trait CatalogServerTrait: Send + Sync {
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
