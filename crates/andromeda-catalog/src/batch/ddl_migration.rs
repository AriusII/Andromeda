//! Catalog-level DDL migration planning evidence derived from DefinitionBatch dry-runs.
//!
//! This module intentionally does **not** introduce a SQL DDL surface and does
//! not apply catalog mutations. It is a reporting layer over an already-validated
//! [`DefinitionBatchPlan`], keeping catalog object migration planning distinct
//! from SRPL procedure ALTER/DROP lifecycle helpers.

use andromeda_core::{CatalogObjectId, CatalogVersion, DatabaseId, NamespaceId};

use crate::{ObjectKind, QualifiedName};

use super::{
    CatalogMutationOperation, CatalogPublicationSemantics, DefinitionBatchId, DefinitionBatchPlan,
};

/// Classification for a planned definition-batch operation when viewed as DDL
/// migration evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogDdlMigrationClassification {
    /// Catalog object definition evolution owned by the catalog engine.
    CatalogDdlMigration,
    /// SRPL procedure source lifecycle transition, owned by the SRPL bridge and
    /// DEC-022/DEC-023 ALTER/DROP semantics rather than generic catalog DDL.
    SrplProcedureLifecycle,
}

/// Object-level action represented in a catalog DDL migration report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogDdlMigrationAction {
    CreateObject,
    DeprecateObject,
}

/// One planned object action in a catalog DDL migration report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDdlMigrationOperation {
    pub operation_index: usize,
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
    pub action: CatalogDdlMigrationAction,
    pub planned_version: CatalogVersion,
    pub classification: CatalogDdlMigrationClassification,
}

impl CatalogDdlMigrationOperation {
    fn new(
        operation_index: usize,
        object_id: CatalogObjectId,
        name: QualifiedName,
        kind: ObjectKind,
        action: CatalogDdlMigrationAction,
        planned_version: CatalogVersion,
        classification: CatalogDdlMigrationClassification,
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

/// Dry-run report for a catalog-level DDL migration plan.
///
/// The report is derived from [`DefinitionBatchPlan`] so it inherits the
/// existing transactional/WAL doctrine: callers must persist the
/// `CatalogMutationPlan::records()` emitted by the source DefinitionBatch plan
/// and publish only after durable commit evidence exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDdlMigrationPlan {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub operation_count: usize,
    /// All object operations that are catalog DDL migration work.
    pub catalog_operations: Vec<CatalogDdlMigrationOperation>,
    /// Procedure lifecycle operations excluded from generic catalog DDL
    /// migration handling and owned by SRPL ALTER/DROP semantics.
    pub procedure_lifecycle_operations: Vec<CatalogDdlMigrationOperation>,
    /// WAL records emitted by the source mutation plan: Begin + Apply* + Commit.
    pub wal_record_count: usize,
    /// Publication semantics required by the source mutation plan.
    pub publication_semantics: CatalogPublicationSemantics,
}

impl CatalogDdlMigrationPlan {
    /// Build a catalog DDL migration report from a successful DefinitionBatch dry-run.
    pub fn from_definition_batch_plan(plan: &DefinitionBatchPlan) -> Self {
        let mut catalog_operations = Vec::with_capacity(plan.mutation_plan.deltas.len());
        let mut procedure_lifecycle_operations = Vec::new();

        for delta in &plan.mutation_plan.deltas {
            let operation = match &delta.operation {
                CatalogMutationOperation::CreateObject { object, .. } => {
                    CatalogDdlMigrationOperation::new(
                        delta.operation_index,
                        object.object_id,
                        object.name.clone(),
                        object.kind,
                        CatalogDdlMigrationAction::CreateObject,
                        delta.planned_version,
                        CatalogDdlMigrationClassification::CatalogDdlMigration,
                    )
                }
                CatalogMutationOperation::DeprecateObject { target } => {
                    CatalogDdlMigrationOperation::new(
                        delta.operation_index,
                        target.object.object_id,
                        target.object.name.clone(),
                        target.object.kind,
                        CatalogDdlMigrationAction::DeprecateObject,
                        delta.planned_version,
                        if target.object.kind == ObjectKind::Procedure {
                            CatalogDdlMigrationClassification::SrplProcedureLifecycle
                        } else {
                            CatalogDdlMigrationClassification::CatalogDdlMigration
                        },
                    )
                }
            };
            match operation.classification {
                CatalogDdlMigrationClassification::CatalogDdlMigration => {
                    catalog_operations.push(operation)
                }
                CatalogDdlMigrationClassification::SrplProcedureLifecycle => {
                    procedure_lifecycle_operations.push(operation)
                }
            }
        }

        Self {
            batch_id: plan.batch_id,
            database_id: plan.database_id,
            namespace_id: plan.namespace_id,
            previous_version: plan.previous_version,
            next_version: plan.next_version,
            operation_count: plan.operation_count,
            catalog_operations,
            procedure_lifecycle_operations,
            wal_record_count: plan.mutation_plan.record_count(),
            publication_semantics: plan.mutation_plan.publication_semantics,
        }
    }

    /// Returns true when no SRPL ALTER/DROP lifecycle operation is present.
    pub fn is_catalog_only(&self) -> bool {
        self.procedure_lifecycle_operations.is_empty()
    }
}

impl DefinitionBatchPlan {
    /// Returns catalog-level DDL migration evidence for this dry-run plan.
    pub fn catalog_ddl_migration_plan(&self) -> CatalogDdlMigrationPlan {
        CatalogDdlMigrationPlan::from_definition_batch_plan(self)
    }
}
