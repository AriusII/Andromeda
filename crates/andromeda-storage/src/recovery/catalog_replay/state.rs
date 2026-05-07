use std::collections::BTreeSet;

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
};

use crate::wal_record_catalog::CatalogWalRecord;

use super::CatalogSnapshot;

trait CatalogWalRecordReplayExt {
    fn primary_procedure_id(&self) -> Option<CatalogObjectId>;
}

impl CatalogWalRecordReplayExt for CatalogWalRecord {
    fn primary_procedure_id(&self) -> Option<CatalogObjectId> {
        match self {
            CatalogWalRecord::ProcedureAdded { procedure_id, .. }
            | CatalogWalRecord::ProcedureAltered { procedure_id, .. }
            | CatalogWalRecord::ProcedureDropped { procedure_id, .. } => Some(*procedure_id),
            CatalogWalRecord::DefinitionBatchApplied { .. }
            | CatalogWalRecord::StatisticsUpdated { .. }
            | CatalogWalRecord::CatalogCheckpoint { .. } => None,
        }
    }
}

#[derive(Debug)]
pub(super) struct CatalogReplayState {
    snapshot: CatalogSnapshot,
    last_version: Option<CatalogVersion>,
    batch_window: CatalogBatchWindow,
}

impl CatalogReplayState {
    pub(super) fn new() -> Self {
        Self {
            snapshot: CatalogSnapshot::new(CatalogVersion::new(0)),
            last_version: None,
            batch_window: CatalogBatchWindow::new(),
        }
    }

    pub(super) fn into_snapshot(self) -> CatalogSnapshot {
        self.snapshot
    }

    pub(super) fn record_is_beyond_target(
        &self,
        record: &CatalogWalRecord,
        target_catalog_version: CatalogVersion,
    ) -> bool {
        record
            .catalog_version()
            .is_some_and(|version| version.get() > target_catalog_version.get())
    }

    pub(super) fn validate_and_advance_version(
        &mut self,
        record: &CatalogWalRecord,
    ) -> AndromedaResult<()> {
        let Some(record_version) = record.catalog_version() else {
            return Ok(());
        };

        if matches!(record, CatalogWalRecord::CatalogCheckpoint { .. }) {
            return Ok(());
        }

        self.validate_version_order(record, record_version)?;
        self.last_version = Some(record_version);
        self.snapshot.catalog_version = record_version;
        self.batch_window.observe_record(record, record_version);
        Ok(())
    }

    fn validate_version_order(
        &self,
        record: &CatalogWalRecord,
        record_version: CatalogVersion,
    ) -> AndromedaResult<()> {
        let Some(prev_version) = self.last_version else {
            return Ok(());
        };

        if record_version.get() < prev_version.get()
            || (record_version.get() == prev_version.get()
                && !self
                    .batch_window
                    .accepts_equal_version_detail(record, record_version))
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "catalog version reordering: previous {}, current {}",
                    prev_version.get(),
                    record_version.get()
                ),
            ));
        }

        Ok(())
    }

    pub(super) fn replay_record(&mut self, record: &CatalogWalRecord) -> AndromedaResult<()> {
        match record {
            CatalogWalRecord::DefinitionBatchApplied {
                affected_procedure_ids,
                ..
            } => {
                self.replay_definition_batch_applied(affected_procedure_ids);
                Ok(())
            }
            CatalogWalRecord::ProcedureAdded { procedure_id, .. } => {
                self.mark_procedure_visible(*procedure_id);
                Ok(())
            }
            CatalogWalRecord::ProcedureAltered { procedure_id, .. } => {
                self.ensure_visible_procedure(*procedure_id, "ProcedureAltered")?;
                self.snapshot
                    .procedure_versions
                    .insert(*procedure_id, self.snapshot.catalog_version);
                Ok(())
            }
            CatalogWalRecord::ProcedureDropped { procedure_id, .. } => {
                self.ensure_visible_procedure(*procedure_id, "ProcedureDropped")?;
                self.snapshot.procedure_ids.remove(procedure_id);
                self.snapshot
                    .procedure_versions
                    .insert(*procedure_id, self.snapshot.catalog_version);
                Ok(())
            }
            CatalogWalRecord::StatisticsUpdated {
                table_id,
                column_id,
                ..
            } => {
                let _ = (table_id, column_id);
                Ok(())
            }
            CatalogWalRecord::CatalogCheckpoint {
                visible_procedure_count,
                ..
            } => self.validate_checkpoint(*visible_procedure_count),
        }
    }

    fn replay_definition_batch_applied(&mut self, affected_procedure_ids: &[CatalogObjectId]) {
        for pid in affected_procedure_ids {
            self.mark_procedure_visible(*pid);
        }
    }

    fn mark_procedure_visible(&mut self, procedure_id: CatalogObjectId) {
        self.snapshot.procedure_ids.insert(procedure_id);
        self.snapshot
            .procedure_versions
            .insert(procedure_id, self.snapshot.catalog_version);
    }

    fn ensure_visible_procedure(
        &self,
        procedure_id: CatalogObjectId,
        operation: &str,
    ) -> AndromedaResult<()> {
        if self.snapshot.procedure_ids.contains(&procedure_id) {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "{operation} references non-existent procedure: {}",
                procedure_id.get()
            ),
        ))
    }

    fn validate_checkpoint(&self, visible_procedure_count: usize) -> AndromedaResult<()> {
        if self.snapshot.visible_procedure_count() == visible_procedure_count {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "CatalogCheckpoint visible_procedure_count mismatch: \
                 checkpoint says {}, replay has {}",
                visible_procedure_count,
                self.snapshot.visible_procedure_count()
            ),
        ))
    }
}

#[derive(Debug)]
struct CatalogBatchWindow {
    version: Option<CatalogVersion>,
    pending_objects: BTreeSet<CatalogObjectId>,
}

impl CatalogBatchWindow {
    fn new() -> Self {
        Self {
            version: None,
            pending_objects: BTreeSet::new(),
        }
    }

    fn accepts_equal_version_detail(
        &self,
        record: &CatalogWalRecord,
        record_version: CatalogVersion,
    ) -> bool {
        self.version == Some(record_version)
            && record
                .primary_procedure_id()
                .is_some_and(|id| self.pending_objects.contains(&id))
    }

    fn observe_record(&mut self, record: &CatalogWalRecord, record_version: CatalogVersion) {
        if let CatalogWalRecord::DefinitionBatchApplied {
            affected_procedure_ids,
            ..
        } = record
        {
            self.version = Some(record_version);
            self.pending_objects = affected_procedure_ids.iter().copied().collect();
        } else if record
            .primary_procedure_id()
            .is_some_and(|id| self.pending_objects.remove(&id))
            && self.pending_objects.is_empty()
        {
            self.version = None;
        }
    }
}
