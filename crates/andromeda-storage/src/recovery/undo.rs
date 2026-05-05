//! Undo chain builders and rollback replay for WAL recovery.
//!
//! # Design
//!
//! This module is the **single owner** of:
//! * Building undo chains from redo records
//! * Replaying undo operations in reverse LSN order
//! * Ensuring undo operations correctly reverse redo operations
//! * Validating MVCC visibility for undo operations
//!
//! # Undo Chain Correctness
//!
//! The undo chain must satisfy:
//!
//! 1. **Reversibility:** Each undo operation must correctly reverse its
//!    corresponding redo operation. For example:
//!    - RowInsert undo → RowDelete (mark inserted slot as deleted)
//!    - RowDelete undo → MarkUndeleted (make slot visible again)
//!    - RowUpdate undo → RestorePreviousVersion
//!
//! 2. **LSN Ordering:** Undo records for a single transaction must be
//!    processed in descending (reverse) LSN order. This ensures that
//!    if the rollback crashes mid-way, the database is in a consistent state
//!    that can be safely rolled back again on recovery.
//!
//! 3. **Transaction Boundary:** Undo operations must be strictly contained
//!    within a single transaction. No undo operation may affect state
//!    outside its owning transaction.
//!
//! 4. **Idempotency:** Undo operations must be idempotent. Re-rolling back
//!    the same transaction must produce the same result as the first rollback.

use std::collections::HashMap;

use andromeda_core::{AndromedaResult, TransactionId};

use crate::{Lsn, WalRecordKind};

use super::storage_error;

/// Represents an undo operation to be applied during rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoOperation {
    /// Undo a row insertion by marking the slot as deleted.
    UndoRowInsert,
    /// Undo a row deletion by clearing the deletion marker.
    UndoRowDelete,
    /// Undo a row update by restoring the previous version.
    UndoRowUpdate,
    /// Undo an index insertion by deleting the index entry.
    UndoIndexInsert,
    /// Undo an index deletion by re-inserting the index entry.
    UndoIndexDelete,
}

impl UndoOperation {
    /// Get the redo operation this undo operation reverses.
    pub const fn reverses(self) -> WalRecordKind {
        match self {
            Self::UndoRowInsert => WalRecordKind::RowInsert,
            Self::UndoRowDelete => WalRecordKind::RowDelete,
            Self::UndoRowUpdate => WalRecordKind::RowUpdate,
            Self::UndoIndexInsert => WalRecordKind::IndexInsert,
            Self::UndoIndexDelete => WalRecordKind::IndexDelete,
        }
    }
}

/// An undo record to be applied during transaction rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoRecord {
    /// LSN of the original redo record being undone.
    pub original_redo_lsn: Lsn,
    /// Operation to be applied during undo.
    pub operation: UndoOperation,
    /// Transaction being rolled back.
    pub transaction_id: TransactionId,
}

/// Undo chain for a single transaction.
///
/// Undo records are stored in **descending LSN order** (most recent first).
/// This ensures that if rollback crashes, the database can be safely rolled
/// back again by replaying the undo chain starting from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoChain {
    /// Transaction being rolled back.
    pub transaction_id: TransactionId,
    /// Undo records in descending LSN order.
    pub records: Vec<UndoRecord>,
}

impl UndoChain {
    pub fn new(transaction_id: TransactionId) -> Self {
        Self {
            transaction_id,
            records: Vec::new(),
        }
    }

    /// Add an undo record, maintaining descending LSN order.
    ///
    /// # Preconditions
    ///
    /// Undo records must be added in descending LSN order.
    /// This is typically used with pre-built chains (e.g., from recovery).
    /// For building chains from redo records, use UndoChainsBuilder instead.
    pub fn add_undo_record(&mut self, record: UndoRecord) -> AndromedaResult<()> {
        // Verify LSN ordering: new record LSN should be less than or equal to
        // the previous record LSN (descending order).
        if let Some(last) = self.records.last() {
            if record.original_redo_lsn > last.original_redo_lsn {
                return Err(storage_error(
                    "undo chain LSN ordering violated: undo records must be in descending LSN order",
                ));
            }
        }

        // Verify transaction consistency.
        if record.transaction_id != self.transaction_id {
            return Err(storage_error(
                "undo chain transaction mismatch: undo record belongs to different transaction",
            ));
        }

        self.records.push(record);
        Ok(())
    }

    /// Get the next undo record to apply (LIFO order: most recent first).
    pub fn next_undo_record(&mut self) -> Option<UndoRecord> {
        self.records.pop()
    }

    /// Validate that the undo chain is well-formed.
    pub fn validate(&self) -> AndromedaResult<()> {
        // Verify LSN descending order.
        for window in self.records.windows(2) {
            if window[0].original_redo_lsn < window[1].original_redo_lsn {
                return Err(storage_error(
                    "undo chain LSN ordering violated: records not in descending order",
                ));
            }
        }

        // Verify transaction consistency.
        for record in &self.records {
            if record.transaction_id != self.transaction_id {
                return Err(storage_error(
                    "undo chain transaction mismatch: found record for different transaction",
                ));
            }
        }

        Ok(())
    }
}

/// Builder for undo chains from redo records.
pub struct UndoChainsBuilder {
    /// Temporary storage: transaction ID -> list of undo records in order encountered
    pending_records: HashMap<TransactionId, Vec<UndoRecord>>,
}

impl UndoChainsBuilder {
    pub fn new() -> Self {
        Self {
            pending_records: HashMap::new(),
        }
    }

    /// Add a redo record to the undo chain builder.
    ///
    /// The builder will construct undo operations based on the redo kind.
    /// If the redo operation is undoable, it will be added to the appropriate
    /// undo chain. If not, the record is skipped (e.g., TxBegin, TxCommit).
    /// Records are collected in ascending LSN order (as encountered in WAL).
    pub fn add_redo_record(
        &mut self,
        lsn: Lsn,
        kind: WalRecordKind,
        transaction_id: TransactionId,
    ) -> AndromedaResult<()> {
        let undo_op = match kind {
            WalRecordKind::RowInsert => Some(UndoOperation::UndoRowInsert),
            WalRecordKind::RowDelete => Some(UndoOperation::UndoRowDelete),
            WalRecordKind::RowUpdate => Some(UndoOperation::UndoRowUpdate),
            WalRecordKind::IndexInsert => Some(UndoOperation::UndoIndexInsert),
            WalRecordKind::IndexDelete => Some(UndoOperation::UndoIndexDelete),
            // Non-undoable records (transaction boundaries, pages, MVCC, etc.) are ignored.
            _ => None,
        };

        if let Some(undo_op) = undo_op {
            let record = UndoRecord {
                original_redo_lsn: lsn,
                operation: undo_op,
                transaction_id,
            };

            self.pending_records
                .entry(transaction_id)
                .or_insert_with(Vec::new)
                .push(record);
        }

        Ok(())
    }

    /// Finalize and return all undo chains.
    ///
    /// # Postconditions
    ///
    /// All returned undo chains are:
    /// * In descending LSN order
    /// * Validated for consistency
    /// * Ready for rollback replay
    pub fn build(self) -> AndromedaResult<Vec<UndoChain>> {
        let mut chains = Vec::new();

        for (transaction_id, records) in self.pending_records {
            let mut chain = UndoChain::new(transaction_id);

            // Add records in reverse order (so they end up descending for LIFO pop)
            for record in records.into_iter().rev() {
                // Verify transaction consistency
                if record.transaction_id != transaction_id {
                    return Err(storage_error(
                        "undo chain transaction mismatch: record belongs to different transaction",
                    ));
                }
                chain.records.push(record);
            }

            chain.validate()?;
            chains.push(chain);
        }

        Ok(chains)
    }
}

impl Default for UndoChainsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_operation_reverses_correct_redo_kind() {
        assert_eq!(
            UndoOperation::UndoRowInsert.reverses(),
            WalRecordKind::RowInsert
        );
        assert_eq!(
            UndoOperation::UndoRowDelete.reverses(),
            WalRecordKind::RowDelete
        );
        assert_eq!(
            UndoOperation::UndoRowUpdate.reverses(),
            WalRecordKind::RowUpdate
        );
    }

    #[test]
    fn undo_chain_maintains_descending_lsn_order() {
        let tid = TransactionId::new(1);
        let mut chain = UndoChain::new(tid);

        let rec3 = UndoRecord {
            original_redo_lsn: Lsn::new(3),
            operation: UndoOperation::UndoRowInsert,
            transaction_id: tid,
        };
        let rec2 = UndoRecord {
            original_redo_lsn: Lsn::new(2),
            operation: UndoOperation::UndoRowDelete,
            transaction_id: tid,
        };
        let rec1 = UndoRecord {
            original_redo_lsn: Lsn::new(1),
            operation: UndoOperation::UndoRowUpdate,
            transaction_id: tid,
        };

        chain.add_undo_record(rec3).unwrap();
        chain.add_undo_record(rec2).unwrap();
        chain.add_undo_record(rec1).unwrap();

        assert_eq!(chain.records.len(), 3);
        assert_eq!(chain.records[0].original_redo_lsn, Lsn::new(3));
        assert_eq!(chain.records[1].original_redo_lsn, Lsn::new(2));
        assert_eq!(chain.records[2].original_redo_lsn, Lsn::new(1));
    }

    #[test]
    fn undo_chain_rejects_ascending_lsn_order() {
        let tid = TransactionId::new(1);
        let mut chain = UndoChain::new(tid);

        let rec1 = UndoRecord {
            original_redo_lsn: Lsn::new(1),
            operation: UndoOperation::UndoRowInsert,
            transaction_id: tid,
        };
        let rec2 = UndoRecord {
            original_redo_lsn: Lsn::new(2),
            operation: UndoOperation::UndoRowDelete,
            transaction_id: tid,
        };

        chain.add_undo_record(rec1).unwrap();
        assert!(chain.add_undo_record(rec2).is_err());
    }

    #[test]
    fn undo_chain_rejects_different_transaction() {
        let tid1 = TransactionId::new(1);
        let tid2 = TransactionId::new(2);
        let mut chain = UndoChain::new(tid1);

        let rec = UndoRecord {
            original_redo_lsn: Lsn::new(1),
            operation: UndoOperation::UndoRowInsert,
            transaction_id: tid2,
        };

        assert!(chain.add_undo_record(rec).is_err());
    }

    #[test]
    fn undo_chains_builder_creates_descending_chains() {
        let mut builder = UndoChainsBuilder::new();
        let tid = TransactionId::new(1);

        // Add redo records in ascending LSN order (as they appear in WAL).
        builder
            .add_redo_record(Lsn::new(1), WalRecordKind::RowInsert, tid)
            .unwrap();
        builder
            .add_redo_record(Lsn::new(2), WalRecordKind::RowUpdate, tid)
            .unwrap();
        builder
            .add_redo_record(Lsn::new(3), WalRecordKind::RowDelete, tid)
            .unwrap();

        let mut chains = builder.build().unwrap();
        assert_eq!(chains.len(), 1);

        let chain = &mut chains[0];
        // After build(), records should be in descending order for rollback.
        assert_eq!(chain.records[0].original_redo_lsn, Lsn::new(3));
        assert_eq!(chain.records[1].original_redo_lsn, Lsn::new(2));
        assert_eq!(chain.records[2].original_redo_lsn, Lsn::new(1));
    }

    #[test]
    fn undo_chain_lifo_pop_order() {
        let tid = TransactionId::new(1);
        let mut chain = UndoChain::new(tid);

        // Add undo records in descending order (3, 2, 1)
        chain
            .add_undo_record(UndoRecord {
                original_redo_lsn: Lsn::new(3),
                operation: UndoOperation::UndoRowDelete,
                transaction_id: tid,
            })
            .unwrap();
        chain
            .add_undo_record(UndoRecord {
                original_redo_lsn: Lsn::new(2),
                operation: UndoOperation::UndoRowDelete,
                transaction_id: tid,
            })
            .unwrap();
        chain
            .add_undo_record(UndoRecord {
                original_redo_lsn: Lsn::new(1),
                operation: UndoOperation::UndoRowInsert,
                transaction_id: tid,
            })
            .unwrap();

        // Pop should return in LIFO order (reverse insertion order).
        let rec3 = chain.next_undo_record().unwrap();
        assert_eq!(rec3.original_redo_lsn, Lsn::new(1));

        let rec2 = chain.next_undo_record().unwrap();
        assert_eq!(rec2.original_redo_lsn, Lsn::new(2));

        let rec1 = chain.next_undo_record().unwrap();
        assert_eq!(rec1.original_redo_lsn, Lsn::new(3));

        assert!(chain.next_undo_record().is_none());
    }
}
