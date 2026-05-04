//! WAL public compatibility surface.
//!
//! The implementation lives under [`crate::write_ahead_log`].  Keep the public
//! WAL items available at the crate root for legacy imports, but do not re-export
//! the domain modules themselves. Re-exporting the modules via a glob would make
//! `write_ahead_log::segment` collide with the crate's storage `segment` module.

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
