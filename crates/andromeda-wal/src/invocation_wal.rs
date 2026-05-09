use andromeda_error::AndromedaResult;
use andromeda_types::TransactionId;

use crate::Lsn;
use crate::file_wal::FileWal;
use crate::write_ahead_log::{InMemoryWal, WalRecordKind};

/// Synchronous WAL boundary used by local invocation runtimes.
pub trait InvocationWal {
    fn append(
        &mut self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: &[u8],
    ) -> AndromedaResult<Lsn>;

    fn flush_through(&mut self, lsn: Lsn) -> AndromedaResult<Lsn>;
}

impl InvocationWal for InMemoryWal {
    fn append(
        &mut self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: &[u8],
    ) -> AndromedaResult<Lsn> {
        self.append_payload(kind, transaction_id, payload.to_vec())
    }

    fn flush_through(&mut self, lsn: Lsn) -> AndromedaResult<Lsn> {
        InMemoryWal::flush_through(self, lsn)
    }
}

impl InvocationWal for FileWal {
    fn append(
        &mut self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: &[u8],
    ) -> AndromedaResult<Lsn> {
        self.append_payload(kind, transaction_id, payload.to_vec())
    }

    fn flush_through(&mut self, lsn: Lsn) -> AndromedaResult<Lsn> {
        FileWal::flush_through(self, lsn)
    }
}
