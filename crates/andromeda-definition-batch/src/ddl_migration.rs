//! DDL migration evidence derived from portable DefinitionBatch dry-runs.

use andromeda_catalog_store::{CatalogPublicationSemantics, ObjectKind, QualifiedName};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogObjectId, CatalogVersion, DatabaseId, NamespaceId};

use crate::{
    DefinitionBatch, DefinitionBatchDryRun, DefinitionBatchId, DefinitionOperation,
    dry_run_definition_batch,
};

/// Classification for a planned definition-batch operation when viewed as DDL
/// migration evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionBatchDdlMigrationClassification {
    /// Catalog object definition evolution owned by catalog DDL migration.
    CatalogDdlMigration,
    /// SRPL procedure source lifecycle transition owned by SRPL ALTER/DROP
    /// semantics rather than generic catalog DDL.
    SrplProcedureLifecycle,
}

/// Object-level action represented in a DefinitionBatch DDL migration report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionBatchDdlMigrationAction {
    CreateObject,
    DeprecateObject,
}

/// One planned object action in a DefinitionBatch DDL migration report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBatchDdlMigrationOperation {
    pub operation_index: usize,
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
    pub action: DefinitionBatchDdlMigrationAction,
    pub planned_version: CatalogVersion,
    pub classification: DefinitionBatchDdlMigrationClassification,
}

impl DefinitionBatchDdlMigrationOperation {
    fn new(
        operation_index: usize,
        object_id: CatalogObjectId,
        name: QualifiedName,
        kind: ObjectKind,
        action: DefinitionBatchDdlMigrationAction,
        planned_version: CatalogVersion,
        classification: DefinitionBatchDdlMigrationClassification,
    ) -> Self {
        Self {
            operation_index,
            object_id,
            name,
            kind,
            action,
            planned_version,
            classification,
        }
    }
}

/// Durable publication boundary inherited by a DefinitionBatch DDL migration
/// report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionBatchDdlMigrationBoundary {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    /// WAL records emitted by a catalog mutation apply path: Begin + Apply* +
    /// Commit.
    pub wal_record_count: usize,
    /// Publication semantics required before the planned definitions become
    /// visible.
    pub publication_semantics: CatalogPublicationSemantics,
}

/// Dry-run report for DefinitionBatch DDL migration planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBatchDdlMigrationPlan {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub operation_count: usize,
    /// All object operations that are catalog DDL migration work.
    pub catalog_operations: Vec<DefinitionBatchDdlMigrationOperation>,
    /// Procedure lifecycle operations excluded from generic catalog DDL
    /// migration handling.
    pub procedure_lifecycle_operations: Vec<DefinitionBatchDdlMigrationOperation>,
    /// WAL records emitted by a catalog mutation apply path: Begin + Apply* +
    /// Commit.
    pub wal_record_count: usize,
    /// Publication semantics required before the planned definitions become
    /// visible.
    pub publication_semantics: CatalogPublicationSemantics,
}

impl DefinitionBatchDdlMigrationPlan {
    /// Build DDL migration evidence from a successful portable DefinitionBatch
    /// dry-run and the original ordered operations.
    pub fn from_definition_batch_dry_run(
        dry_run: &DefinitionBatchDryRun,
        operations: &[DefinitionOperation],
    ) -> AndromedaResult<Self> {
        if dry_run.operation_count != operations.len() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch DDL migration operation count must match the dry-run",
            ));
        }

        let mut catalog_operations = Vec::with_capacity(operations.len());
        let mut procedure_lifecycle_operations = Vec::new();

        for (operation_index, operation) in operations.iter().enumerate() {
            let migration_operation = match operation {
                DefinitionOperation::Create(definition) => {
                    let object = definition.object_ref();
                    DefinitionBatchDdlMigrationOperation::new(
                        operation_index,
                        object.object_id,
                        object.name.clone(),
                        object.kind,
                        DefinitionBatchDdlMigrationAction::CreateObject,
                        dry_run.next_version,
                        DefinitionBatchDdlMigrationClassification::CatalogDdlMigration,
                    )
                },
                DefinitionOperation::Deprecate(target) => {
                    DefinitionBatchDdlMigrationOperation::new(
                        operation_index,
                        target.object.object_id,
                        target.object.name.clone(),
                        target.object.kind,
                        DefinitionBatchDdlMigrationAction::DeprecateObject,
                        dry_run.next_version,
                        if target.object.kind == ObjectKind::Procedure {
                            DefinitionBatchDdlMigrationClassification::SrplProcedureLifecycle
                        } else {
                            DefinitionBatchDdlMigrationClassification::CatalogDdlMigration
                        },
                    )
                },
            };

            match migration_operation.classification {
                DefinitionBatchDdlMigrationClassification::CatalogDdlMigration => {
                    catalog_operations.push(migration_operation)
                },
                DefinitionBatchDdlMigrationClassification::SrplProcedureLifecycle => {
                    procedure_lifecycle_operations.push(migration_operation)
                },
            }
        }

        Ok(Self {
            batch_id: dry_run.batch_id,
            database_id: dry_run.database_id,
            namespace_id: dry_run.namespace_id,
            previous_version: dry_run.previous_version,
            next_version: dry_run.next_version,
            operation_count: dry_run.operation_count,
            catalog_operations,
            procedure_lifecycle_operations,
            wal_record_count: operations.len() + 2,
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
        })
    }

    /// Returns true when no SRPL ALTER/DROP lifecycle operation is present.
    pub fn is_catalog_only(&self) -> bool {
        self.procedure_lifecycle_operations.is_empty()
    }

    pub fn boundary(&self) -> DefinitionBatchDdlMigrationBoundary {
        DefinitionBatchDdlMigrationBoundary {
            batch_id: self.batch_id,
            database_id: self.database_id,
            namespace_id: self.namespace_id,
            previous_version: self.previous_version,
            next_version: self.next_version,
            wal_record_count: self.wal_record_count,
            publication_semantics: self.publication_semantics,
        }
    }
}

impl DefinitionBatch {
    /// Validates this batch and returns DefinitionBatch-owned DDL migration
    /// evidence without requiring the catalog runtime mutation planner.
    pub fn ddl_migration_plan(&self) -> AndromedaResult<DefinitionBatchDdlMigrationPlan> {
        let dry_run = dry_run_definition_batch(
            self.batch_id,
            self.database_id,
            self.namespace_id,
            self.base_version,
            &self.operations,
        )?;
        DefinitionBatchDdlMigrationPlan::from_definition_batch_dry_run(&dry_run, &self.operations)
    }
}
