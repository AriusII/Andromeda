//! Catalog Server boundary traits and contracts.
//!
//! This module defines the trait boundaries for the Catalog Server component,
//! enabling remote clients to query procedure metadata, track catalog versions,
//! and subscribe to catalog changes without executing application traffic.
//!
//! ## Contract Overview
//!
//! The Catalog Server provides three primary operations:
//! 1. **Procedure Resolution**: Fetch procedure manifests by ID
//! 2. **Version Tracking**: Query current catalog version
//! 3. **Change Subscriptions**: Subscribe to catalog version updates
//!
//! ### Key Invariants
//!
//! - **No Ad Hoc SQL**: Catalog server queries only pre-authorized catalog views
//! - **LSN-Aware Invalidation**: Changes propagate via log sequence number (LSN) boundaries
//! - **Deterministic Hashing**: Procedure contracts use stable contract hashes
//! - **Error Observability**: All failures returned via `AndromedaResult<T>`
//! - **Remote-First Design**: Clients query metadata independent of application execution
//!
//! ## Usage Pattern
//!
//! ```ignore
//! use andromeda_catalog::server::{CatalogServerTrait, CatalogVersion};
//! use andromeda_core::ProcedureId;
//!
//! let server = MockCatalogServer::new();
//! let procedure = server.resolve_procedure(ProcedureId::new(1))?;
//! let version = server.get_catalog_version();
//! let mut subscription = server.subscribe_to_changes()?;
//! while let Some(change) = subscription.next_change().await {
//!     println!("Catalog updated to version: {}", change.new_version.get());
//! }
//! ```

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ProcedureId,
};
use std::sync::Arc;

/// A procedure manifest providing metadata needed by remote clients.
///
/// Contains the essential information for resolving and executing procedures
/// without requiring ad hoc SQL queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureManifest {
    /// The procedure ID
    pub procedure_id: ProcedureId,
    /// The qualified name (e.g., "schema.procedure_name")
    pub qualified_name: String,
    /// The catalog version at which this procedure is defined
    pub catalog_version: CatalogVersion,
    /// Stable hash of the procedure contract for compatibility checking
    pub contract_hash: Vec<u8>,
    /// Input parameter names and types (schema)
    pub input_schema: Vec<ColumnSchema>,
    /// Output column names and types
    pub output_schema: Vec<ColumnSchema>,
    /// Whether this procedure may modify data
    pub is_mutable: bool,
    /// Minimum catalog version required for compatibility
    pub min_compatible_version: CatalogVersion,
}

impl ProcedureManifest {
    /// Validate the manifest structure.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure id must not be zero",
            ));
        }

        if self.qualified_name.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure qualified name must not be empty",
            ));
        }

        if self.catalog_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog version must not be zero",
            ));
        }

        if self.contract_hash.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "contract hash must not be empty",
            ));
        }

        Ok(())
    }
}

/// A single column in a procedure's input or output schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSchema {
    /// Column name
    pub name: String,
    /// SQL type descriptor (e.g., "int64", "text", "decimal(18,2)")
    pub type_descriptor: String,
    /// Column ordinal (0-based)
    pub ordinal: u32,
    /// Whether NULL values are permitted
    pub nullable: bool,
}

impl ColumnSchema {
    /// Validate the column schema.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column name must not be empty",
            ));
        }

        if self.type_descriptor.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column type descriptor must not be empty",
            ));
        }

        Ok(())
    }
}

/// A change notification for catalog updates.
///
/// Emitted when a procedure or other catalog object changes,
/// allowing clients to invalidate caches and revalidate procedure contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogChangeNotification {
    /// The new catalog version after the change
    pub new_version: CatalogVersion,
    /// The previous catalog version
    pub previous_version: CatalogVersion,
    /// Log sequence number boundary for LSN-aware invalidation
    pub invalidation_boundary_lsn: u64,
}

impl CatalogChangeNotification {
    /// Determine if the change affected a specific procedure.
    /// In a real implementation, this would consult the change journal.
    pub fn affects_procedure(&self, _procedure_id: ProcedureId) -> bool {
        // In mock implementation, any version change affects all procedures
        // Real implementation would consult change tracking metadata
        self.new_version != self.previous_version
    }
}

/// A subscription to catalog changes.
///
/// Clients can poll for changes or await notifications asynchronously.
/// The subscription is automatically invalidated when the server is dropped.
pub trait CatalogChangeSubscription: Send + Sync {
    /// Poll for the next change notification.
    /// Returns `None` if the subscription is closed or no changes are pending.
    fn next_change(&mut self) -> Option<CatalogChangeNotification>;

    /// Check if the subscription is still active.
    fn is_active(&self) -> bool;

    /// Close the subscription explicitly.
    fn close(&mut self);
}

/// Mock implementation of `CatalogChangeSubscription` for testing.
#[derive(Debug, Clone)]
pub struct MockCatalogChangeSubscription {
    changes: Vec<CatalogChangeNotification>,
    index: usize,
    active: bool,
}

impl MockCatalogChangeSubscription {
    pub fn new(changes: Vec<CatalogChangeNotification>) -> Self {
        Self {
            changes,
            index: 0,
            active: true,
        }
    }
}

impl CatalogChangeSubscription for MockCatalogChangeSubscription {
    fn next_change(&mut self) -> Option<CatalogChangeNotification> {
        if !self.active || self.index >= self.changes.len() {
            return None;
        }
        let change = self.changes[self.index];
        self.index += 1;
        Some(change)
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn close(&mut self) {
        self.active = false;
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_procedure_manifest_validation() {
        // Valid manifest
        let valid = ProcedureManifest {
            procedure_id: ProcedureId::new(1),
            qualified_name: "public.my_proc".to_string(),
            catalog_version: CatalogVersion::new(1),
            contract_hash: vec![1, 2, 3],
            input_schema: vec![],
            output_schema: vec![],
            is_mutable: false,
            min_compatible_version: CatalogVersion::new(1),
        };
        assert!(valid.validate().is_ok());

        // Invalid: zero procedure ID
        let invalid_id = ProcedureManifest {
            procedure_id: ProcedureId::new(0),
            ..valid.clone()
        };
        assert!(invalid_id.validate().is_err());

        // Invalid: empty qualified name
        let invalid_name = ProcedureManifest {
            qualified_name: String::new(),
            ..valid.clone()
        };
        assert!(invalid_name.validate().is_err());

        // Invalid: zero catalog version
        let invalid_version = ProcedureManifest {
            catalog_version: CatalogVersion::new(0),
            ..valid.clone()
        };
        assert!(invalid_version.validate().is_err());

        // Invalid: empty contract hash
        let invalid_hash = ProcedureManifest {
            contract_hash: vec![],
            ..valid.clone()
        };
        assert!(invalid_hash.validate().is_err());
    }

    #[test]
    fn test_column_schema_validation() {
        // Valid column
        let valid = ColumnSchema {
            name: "id".to_string(),
            type_descriptor: "int64".to_string(),
            ordinal: 0,
            nullable: false,
        };
        assert!(valid.validate().is_ok());

        // Invalid: empty name
        let invalid_name = ColumnSchema {
            name: String::new(),
            ..valid.clone()
        };
        assert!(invalid_name.validate().is_err());

        // Invalid: empty type descriptor
        let invalid_type = ColumnSchema {
            type_descriptor: String::new(),
            ..valid.clone()
        };
        assert!(invalid_type.validate().is_err());
    }

    #[test]
    fn test_mock_catalog_server_procedure_resolution() {
        let server = MockCatalogServer::new();
        let proc_id = ProcedureId::new(1);

        // Should fail for unknown procedure
        let result = server.resolve_procedure(proc_id);
        assert!(result.is_err());

        // Register a procedure
        let manifest = ProcedureManifest {
            procedure_id: proc_id,
            qualified_name: "public.my_proc".to_string(),
            catalog_version: CatalogVersion::new(1),
            contract_hash: vec![1, 2, 3],
            input_schema: vec![ColumnSchema {
                name: "input_col".to_string(),
                type_descriptor: "int64".to_string(),
                ordinal: 0,
                nullable: false,
            }],
            output_schema: vec![ColumnSchema {
                name: "result".to_string(),
                type_descriptor: "text".to_string(),
                ordinal: 0,
                nullable: true,
            }],
            is_mutable: false,
            min_compatible_version: CatalogVersion::new(1),
        };

        assert!(server.register_procedure(manifest.clone()).is_ok());

        // Now resolution should succeed
        let resolved = server.resolve_procedure(proc_id).unwrap();
        assert_eq!(resolved.qualified_name, "public.my_proc");
        assert_eq!(resolved.procedure_id, proc_id);
    }

    #[test]
    fn test_mock_catalog_server_version_tracking() {
        let server = MockCatalogServer::new();

        let initial_version = server.get_catalog_version();
        assert_eq!(initial_version.get(), 1);

        // Advance version
        assert!(server.advance_catalog_version(100).is_ok());
        let new_version = server.get_catalog_version();
        assert_eq!(new_version.get(), 2);

        // Advance again
        assert!(server.advance_catalog_version(200).is_ok());
        let newer_version = server.get_catalog_version();
        assert_eq!(newer_version.get(), 3);
    }

    #[test]
    fn test_mock_catalog_server_change_subscription() {
        let server = MockCatalogServer::new();

        // Advance version multiple times
        assert!(server.advance_catalog_version(100).is_ok());
        assert!(server.advance_catalog_version(200).is_ok());
        assert!(server.advance_catalog_version(300).is_ok());

        // Subscribe to changes
        let mut subscription = server.subscribe_to_changes().unwrap();
        assert!(subscription.is_active());

        // Poll changes
        let change1 = subscription.next_change();
        assert!(change1.is_some());
        let c1 = change1.unwrap();
        assert_eq!(c1.new_version.get(), 2);
        assert_eq!(c1.previous_version.get(), 1);
        assert_eq!(c1.invalidation_boundary_lsn, 100);

        let change2 = subscription.next_change();
        assert!(change2.is_some());
        let c2 = change2.unwrap();
        assert_eq!(c2.new_version.get(), 3);
        assert_eq!(c2.previous_version.get(), 2);
        assert_eq!(c2.invalidation_boundary_lsn, 200);

        let change3 = subscription.next_change();
        assert!(change3.is_some());
        let c3 = change3.unwrap();
        assert_eq!(c3.new_version.get(), 4);
        assert_eq!(c3.previous_version.get(), 3);
        assert_eq!(c3.invalidation_boundary_lsn, 300);

        // No more changes
        let change4 = subscription.next_change();
        assert!(change4.is_none());

        subscription.close();
        assert!(!subscription.is_active());
    }

    #[test]
    fn test_catalog_change_notification_affects_procedure() {
        let notification = CatalogChangeNotification {
            new_version: CatalogVersion::new(2),
            previous_version: CatalogVersion::new(1),
            invalidation_boundary_lsn: 100,
        };

        // Mock implementation considers any version change as affecting all procedures
        assert!(notification.affects_procedure(ProcedureId::new(1)));
        assert!(notification.affects_procedure(ProcedureId::new(999)));

        let no_change = CatalogChangeNotification {
            new_version: CatalogVersion::new(1),
            previous_version: CatalogVersion::new(1),
            invalidation_boundary_lsn: 100,
        };
        assert!(!no_change.affects_procedure(ProcedureId::new(1)));
    }
}
