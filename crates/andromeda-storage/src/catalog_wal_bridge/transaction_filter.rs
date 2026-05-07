use andromeda_core::{AndromedaResult, TransactionId};

use crate::WalRecord;

use super::error::catalog_wal_error;

pub(super) fn catalog_record_transaction_id(record: &WalRecord) -> AndromedaResult<TransactionId> {
    record.header.transaction_id.ok_or_else(|| {
        catalog_wal_error("catalog WAL publication record requires a transaction id")
    })
}

pub(super) fn require_apply_transaction_matches_begin(
    begin_transaction_id: &TransactionId,
    apply_transaction_id: &TransactionId,
) -> AndromedaResult<()> {
    if begin_transaction_id != apply_transaction_id {
        return Err(catalog_wal_error(
            "catalog WAL apply transaction id does not match publication begin",
        ));
    }

    Ok(())
}

pub(super) fn require_commit_transaction_matches_begin(
    begin_transaction_id: &TransactionId,
    commit_transaction_id: &TransactionId,
) -> AndromedaResult<()> {
    if begin_transaction_id != commit_transaction_id {
        return Err(catalog_wal_error(
            "catalog WAL commit transaction id does not match publication begin",
        ));
    }

    Ok(())
}
