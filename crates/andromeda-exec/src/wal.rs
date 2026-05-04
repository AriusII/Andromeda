use andromeda_core::{AndromedaResult, TransactionId};
use andromeda_storage::{FileWal, InMemoryWal, Lsn, WalRecordKind};

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
