use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_storage_heap::{HeapRowRedoPayloadV1, LocalHeapRowRedoContractBinding};
use andromeda_wal::{Lsn, WalRecordKind};

use super::super::helpers::{
    validate_business_mvcc_timestamp, validate_business_mvcc_transaction_id,
};

/// Commit evidence required before a ProductStock adapter can publish a
/// reservation as visible state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryProductStockCommitEvidence {
    pub transaction_id: TransactionId,
    pub durable_commit_lsn: Lsn,
}

impl InventoryProductStockCommitEvidence {
    pub fn new(transaction_id: TransactionId, durable_commit_lsn: Lsn) -> AndromedaResult<Self> {
        let evidence = Self {
            transaction_id,
            durable_commit_lsn,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn validate(self) -> AndromedaResult<()> {
        validate_business_mvcc_transaction_id(
            self.transaction_id,
            "ProductStock publication requires a non-zero transaction id",
        )?;
        validate_business_mvcc_timestamp(
            self.durable_commit_lsn.get(),
            "ProductStock publication requires a non-zero durable commit LSN",
        )
    }
}

/// Durable heap redo evidence for the ProductStock physical row version.
///
/// `redo_record_lsn` is the WAL record LSN of the HREDOV1 row redo payload.
/// `durable_commit_lsn` proves the transaction commit record was flushed after
/// that redo record, so the row can be made visible by the ProductStock adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryProductStockDurableRedoEvidence {
    pub transaction_id: TransactionId,
    pub redo_record_lsn: Lsn,
    pub durable_commit_lsn: Lsn,
    pub redo_binding: Option<LocalHeapRowRedoContractBinding>,
    pub redo_payload: HeapRowRedoPayloadV1,
}

impl InventoryProductStockDurableRedoEvidence {
    pub fn new(
        transaction_id: TransactionId,
        durable_commit_lsn: Lsn,
        redo_payload: HeapRowRedoPayloadV1,
    ) -> AndromedaResult<Self> {
        Self::new_with_optional_binding(transaction_id, durable_commit_lsn, None, redo_payload)
    }

    pub fn new_with_binding(
        transaction_id: TransactionId,
        durable_commit_lsn: Lsn,
        redo_binding: LocalHeapRowRedoContractBinding,
        redo_payload: HeapRowRedoPayloadV1,
    ) -> AndromedaResult<Self> {
        Self::new_with_optional_binding(
            transaction_id,
            durable_commit_lsn,
            Some(redo_binding),
            redo_payload,
        )
    }

    fn new_with_optional_binding(
        transaction_id: TransactionId,
        durable_commit_lsn: Lsn,
        redo_binding: Option<LocalHeapRowRedoContractBinding>,
        redo_payload: HeapRowRedoPayloadV1,
    ) -> AndromedaResult<Self> {
        let evidence = Self {
            transaction_id,
            redo_record_lsn: redo_payload.resulting_page_lsn(),
            durable_commit_lsn,
            redo_binding,
            redo_payload,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        InventoryProductStockCommitEvidence::new(self.transaction_id, self.durable_commit_lsn)?;
        validate_business_mvcc_timestamp(
            self.redo_record_lsn.get(),
            "ProductStock redo evidence requires a non-zero redo record LSN",
        )?;

        if self.redo_payload.wal_record_kind() != WalRecordKind::RowInsert {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock durable redo evidence must be a heap row insert redo record",
            ));
        }

        if self.redo_payload.resulting_page_lsn() != self.redo_record_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock durable redo evidence LSN must match the HREDOV1 resulting page LSN",
            ));
        }

        if let Some(binding) = self.redo_binding {
            binding.validate()?;
        }

        if self.durable_commit_lsn <= self.redo_record_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock durable commit LSN must follow the row redo record LSN",
            ));
        }

        Ok(())
    }
}
