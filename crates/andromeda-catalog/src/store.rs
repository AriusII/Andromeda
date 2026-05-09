//! Catalog system store and mutation planning.
//!
//! This module provides the `CatalogSystemStore` facade which validates definition batches
//! against the current catalog snapshot and plans mutations. Durable persistence is
//! still delegated to callers, but callers can now provide committed durable evidence
//! to publish the next visible snapshot with a catalog-local receipt.

use andromeda_catalog_store::{
    CatalogStoreApplyReport, CatalogStoreDurableApplyReport, CatalogStoreWalAppend,
    CatalogStoreWalAppendSequenceError, validate_catalog_store_wal_append_sequence,
};
use andromeda_definition_batch::{
    DefinitionBatch, DefinitionBatchDependencyGraphHash, DefinitionBatchSourceHash,
};
use andromeda_error::AndromedaResult;
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use crate::{
    CatalogMutationCommitEvidence, CatalogMutationDurability, CatalogMutationPlan,
    CatalogMutationRecordKind, CatalogPublicationReceipt, CatalogSnapshot,
    CatalogSnapshotApplyReport, DefinitionBatchPlan,
};

/// In-memory catalog system facade for definition planning and snapshot mutation.
///
/// This type intentionally owns no WAL or storage engine. It validates that a
/// [`DefinitionBatch`] is planned against the currently visible snapshot, delegates
/// persistence of [`crate::CatalogMutationRecord`] values to callers that own storage,
/// and only performs durable publication after callers provide committed durable
/// mutation evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSystemStore {
    snapshot: CatalogSnapshot,
}

impl CatalogSystemStore {
    pub fn new(snapshot: CatalogSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn empty(
        database_id: DatabaseId,
        namespace_id: NamespaceId,
        version: CatalogVersion,
    ) -> Self {
        Self::new(CatalogSnapshot::empty(database_id, namespace_id, version))
    }

    pub fn snapshot(&self) -> &CatalogSnapshot {
        &self.snapshot
    }

    pub fn snapshot_mut(&mut self) -> &mut CatalogSnapshot {
        &mut self.snapshot
    }

    pub fn into_snapshot(self) -> CatalogSnapshot {
        self.snapshot
    }

    pub fn plan_definition_batch(
        &self,
        batch: &DefinitionBatch,
    ) -> AndromedaResult<DefinitionBatchPlan> {
        self.snapshot.plan_definition_batch(batch)
    }

    pub fn apply_mutation_plan(
        &mut self,
        plan: &CatalogMutationPlan,
    ) -> AndromedaResult<CatalogSnapshotApplyReport> {
        self.snapshot.apply_mutation_plan(plan)
    }

    pub fn publish_durable_mutation_plan(
        &mut self,
        plan: &CatalogMutationPlan,
        evidence: CatalogMutationCommitEvidence,
    ) -> AndromedaResult<CatalogPublicationReceipt> {
        self.snapshot.publish_durable_mutation_plan(plan, evidence)
    }

    pub fn apply_definition_batch(
        &mut self,
        batch: &DefinitionBatch,
    ) -> AndromedaResult<CatalogSystemApplyReport> {
        let plan = self.plan_definition_batch(batch)?;
        let source_hash = batch.source_hash();
        let dependency_graph_hash = batch.dependency_graph_hash()?;
        let snapshot_report = self.apply_mutation_plan(&plan.mutation_plan)?;

        Ok(CatalogSystemApplyReport {
            plan,
            snapshot_report,
            source_hash,
            dependency_graph_hash,
        })
    }

    /// Plans, WAL-persists, durably flushes, and then publishes a definition
    /// batch.
    ///
    /// The store remains storage-agnostic: callers own the WAL and provide
    /// append/flush callbacks. Publication happens only after every catalog
    /// mutation record has been appended and the returned durable LSN reaches
    /// the commit record LSN.
    pub fn apply_definition_batch_durably<Append, Flush>(
        &mut self,
        batch: &DefinitionBatch,
        mut append: Append,
        mut flush_through: Flush,
    ) -> AndromedaResult<CatalogSystemDurableApplyReport>
    where
        Append: FnMut(CatalogMutationRecordKind, &[u8]) -> AndromedaResult<u64>,
        Flush: FnMut(u64) -> AndromedaResult<u64>,
    {
        let plan = self.plan_definition_batch(batch)?;
        let source_hash = batch.source_hash();
        let dependency_graph_hash = batch.dependency_graph_hash()?;
        let records = plan.mutation_plan.records();
        let expected_kinds = records
            .iter()
            .map(|record| record.kind())
            .collect::<Vec<_>>();
        let mut appended_records = Vec::with_capacity(records.len());

        for record in &records {
            let payload = record.encode_durable_payload()?;
            let kind = record.kind();
            let lsn = append(kind, &payload)?;
            appended_records.push(CatalogStoreWalAppend { kind, lsn });
        }

        validate_catalog_wal_append_sequence(&appended_records, &expected_kinds)?;

        let commit_record = records.last().ok_or_else(|| {
            andromeda_error::AndromedaError::new(
                andromeda_error::AndromedaErrorKind::Catalog,
                "catalog mutation plan must emit a commit record",
            )
        })?;
        let commit_lsn = appended_records
            .last()
            .map(|record| record.lsn)
            .ok_or_else(|| {
                andromeda_error::AndromedaError::new(
                    andromeda_error::AndromedaErrorKind::Catalog,
                    "catalog mutation plan must append at least one record",
                )
            })?;
        let durable_lsn = flush_through(commit_lsn)?;
        let evidence = CatalogMutationCommitEvidence::from_durable_commit_record(
            commit_record,
            records.len(),
            CatalogMutationDurability::StorageWal {
                commit_lsn,
                durable_lsn,
            },
        )?;
        let receipt = self.publish_durable_mutation_plan(&plan.mutation_plan, evidence)?;

        Ok(CatalogStoreDurableApplyReport {
            plan,
            receipt,
            appended_records,
            source_hash,
            dependency_graph_hash,
        })
    }
}

fn validate_catalog_wal_append_sequence(
    appended_records: &[CatalogSystemWalAppend],
    expected_kinds: &[CatalogMutationRecordKind],
) -> AndromedaResult<()> {
    validate_catalog_store_wal_append_sequence(appended_records, expected_kinds).map_err(|error| {
        let message = match error {
            CatalogStoreWalAppendSequenceError::KindMismatch { index } => {
                format!("catalog WAL append kind at index {index} must match the mutation plan")
            }
            CatalogStoreWalAppendSequenceError::ZeroLsn { index } => {
                format!("catalog WAL append LSN at index {index} must not be zero (LSN must not be zero)")
            }
            CatalogStoreWalAppendSequenceError::NonIncreasingLsn { index } => {
                format!(
                    "catalog WAL append LSN at index {index} must be strictly increasing"
                )
            }
            CatalogStoreWalAppendSequenceError::MissingPlannedRecord
            | CatalogStoreWalAppendSequenceError::MissingCommitRecord => error.message().to_string(),
        };

        andromeda_error::AndromedaError::new(
            andromeda_error::AndromedaErrorKind::Catalog,
            message,
        )
    })
}

pub type CatalogSystemApplyReport = CatalogStoreApplyReport<
    DefinitionBatchPlan,
    CatalogSnapshotApplyReport,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>;

pub type CatalogSystemWalAppend = CatalogStoreWalAppend<CatalogMutationRecordKind>;

pub type CatalogSystemDurableApplyReport = CatalogStoreDurableApplyReport<
    DefinitionBatchPlan,
    CatalogPublicationReceipt,
    CatalogMutationRecordKind,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>;

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog_store::{
        CatalogDefinition, CatalogObjectRef, ObjectKind, QualifiedName, TableDefinition,
    };
    use andromeda_definition_batch::{DefinitionBatchId, DefinitionOperation};
    use andromeda_error::AndromedaErrorKind;
    use andromeda_types::{CatalogObjectId, ColumnDescriptor, ScalarType, TypeDescriptor};

    fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    fn table(id: u64, name: &str, version: CatalogVersion) -> TableDefinition {
        TableDefinition {
            object: CatalogObjectRef {
                object_id: CatalogObjectId::new(id),
                name: QualifiedName::parse(name).unwrap(),
                kind: ObjectKind::Table,
                catalog_version: version,
            },
            columns: vec![column("ProductId", 0)],
        }
    }

    fn batch(base_version: CatalogVersion) -> DefinitionBatch {
        DefinitionBatch {
            batch_id: DefinitionBatchId::new(1),
            database_id: DatabaseId::new(10),
            namespace_id: NamespaceId::new(20),
            base_version,
            operations: vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(
                    1,
                    "Inventory.Product",
                    CatalogVersion::new(base_version.get() + 1),
                ),
            ))],
        }
    }

    #[test]
    fn system_store_plans_against_current_snapshot_version() {
        let store = CatalogSystemStore::empty(
            DatabaseId::new(10),
            NamespaceId::new(20),
            CatalogVersion::new(7),
        );

        let error = store
            .plan_definition_batch(&batch(CatalogVersion::new(6)))
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("base version"));
    }

    #[test]
    fn system_store_applies_planned_batch_without_durable_publication() {
        let mut store = CatalogSystemStore::empty(
            DatabaseId::new(10),
            NamespaceId::new(20),
            CatalogVersion::new(7),
        );

        let report = store
            .apply_definition_batch(&batch(CatalogVersion::new(7)))
            .unwrap();

        assert_eq!(report.plan.next_version, CatalogVersion::new(8));
        assert_eq!(report.snapshot_report.next_version, CatalogVersion::new(8));
        assert!(!report.snapshot_report.durable_publication_performed);
        assert_eq!(store.snapshot().version, CatalogVersion::new(8));
    }
}
