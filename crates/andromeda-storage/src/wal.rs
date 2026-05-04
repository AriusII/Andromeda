//! Legacy crate-root WAL compatibility surface.
//!
//! The canonical implementation lives under [`crate::write_ahead_log`]:
//! * record types in [`crate::write_ahead_log::record`],
//! * in-memory WAL manager in [`crate::write_ahead_log::manager`],
//! * transaction classification in [`crate::write_ahead_log::transaction`].
//!
//! These items are re-exported at the crate root for legacy importers. Do not
//! re-export the WAL submodules themselves: `write_ahead_log::segment` would
//! collide with the storage crate's `segment` module.

pub use crate::write_ahead_log::{manager::*, record::*, transaction::*};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_taxonomy_marks_transaction_boundaries() {
        assert!(WalRecordKind::TxBegin.is_transaction_boundary());
        assert!(WalRecordKind::TxCommit.is_transaction_boundary());
        assert!(WalRecordKind::TxRollback.is_transaction_boundary());
        assert!(!WalRecordKind::RowInsert.is_transaction_boundary());
    }
}
