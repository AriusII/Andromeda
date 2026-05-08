use crate::{WalRecord, WalRecordKind};
use andromeda_core::AndromedaResult;

use super::{constants::WAL_BATCH_ROW_LIMIT, errors::storage_error};

const fn is_transaction_row_operation(kind: WalRecordKind) -> bool {
    matches!(
        kind,
        WalRecordKind::RowInsert | WalRecordKind::RowUpdate | WalRecordKind::RowDelete
    )
}

/// Validate transaction row-operation cardinality for a WAL record batch.
///
/// Only row operations count toward the transaction batch limit. Transaction
/// metadata records remain outside this limit.
pub fn validate_transaction_batch_cardinality(records: &[WalRecord]) -> AndromedaResult<usize> {
    let mut row_count = 0usize;

    for record in records {
        if is_transaction_row_operation(record.header.kind) {
            row_count += 1;
            if row_count > WAL_BATCH_ROW_LIMIT {
                return Err(storage_error(format!(
                    "transaction batch size {} exceeds limit {}",
                    row_count, WAL_BATCH_ROW_LIMIT
                )));
            }
        }
    }

    Ok(row_count)
}
