use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, TransactionId,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MvccIsolationPolicy {
    ReadCommitted,
    RepeatableRead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub timestamp: u64,
    pub catalog_version: Option<CatalogVersion>,
    pub isolation_policy: MvccIsolationPolicy,
    pub transaction_id: Option<TransactionId>,
    pub active_tx_ids: Vec<TransactionId>,
}

impl Snapshot {
    pub fn new(timestamp: u64) -> Self {
        Self {
            timestamp,
            catalog_version: None,
            isolation_policy: MvccIsolationPolicy::ReadCommitted,
            transaction_id: None,
            active_tx_ids: Vec::new(),
        }
    }

    pub fn with_context(
        timestamp: u64,
        catalog_version: CatalogVersion,
        isolation_policy: MvccIsolationPolicy,
        transaction_id: Option<TransactionId>,
        active_tx_ids: impl IntoIterator<Item = TransactionId>,
    ) -> AndromedaResult<Self> {
        let mut active_tx_ids: Vec<_> = active_tx_ids.into_iter().collect();
        active_tx_ids.sort_unstable();
        active_tx_ids.dedup();

        let snapshot = Self {
            timestamp,
            catalog_version: Some(catalog_version),
            isolation_policy,
            transaction_id,
            active_tx_ids,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn is_current_transaction(&self, transaction_id: TransactionId) -> bool {
        self.transaction_id == Some(transaction_id)
    }

    pub fn is_transaction_active(&self, transaction_id: TransactionId) -> bool {
        self.active_tx_ids.binary_search(&transaction_id).is_ok()
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.timestamp == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "snapshot timestamp must not be zero",
            ));
        }

        if matches!(self.catalog_version, Some(catalog_version) if catalog_version.get() == 0) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "snapshot catalog version must not be zero",
            ));
        }

        if matches!(self.transaction_id, Some(transaction_id) if transaction_id.get() == 0) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "snapshot transaction id must not be zero",
            ));
        }

        let mut previous = None;
        for transaction_id in &self.active_tx_ids {
            if transaction_id.get() == 0 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "snapshot active transaction id must not be zero",
                ));
            }

            if previous.is_some_and(|previous| previous >= *transaction_id) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "snapshot active transaction ids must be sorted and unique",
                ));
            }

            previous = Some(*transaction_id);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    InFlight,
    Committed,
    RolledBack,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransactionStatusTable {
    statuses: BTreeMap<TransactionId, TransactionStatus>,
}

impl TransactionStatusTable {
    pub fn new() -> Self {
        Self {
            statuses: BTreeMap::new(),
        }
    }

    pub fn record(
        &mut self,
        transaction_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        validate_transaction_id(transaction_id, "transaction status id must not be zero")?;
        self.statuses.insert(transaction_id, status);
        Ok(())
    }

    pub fn status(&self, transaction_id: TransactionId) -> Option<TransactionStatus> {
        self.statuses.get(&transaction_id).copied()
    }

    fn status_for_snapshot(
        &self,
        transaction_id: TransactionId,
        version_ts: u64,
        snapshot: &Snapshot,
    ) -> TransactionStatus {
        self.status(transaction_id).unwrap_or_else(|| {
            if snapshot.is_transaction_active(transaction_id) || version_ts > snapshot.timestamp {
                TransactionStatus::InFlight
            } else {
                TransactionStatus::Committed
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MvccRowHeader {
    pub begin_ts: u64,
    pub end_ts: Option<u64>,
    pub creator_tx_id: TransactionId,
    pub deleter_tx_id: Option<TransactionId>,
    pub previous_version_ptr: Option<u64>,
    pub flags: u32,
}

impl MvccRowHeader {
    pub fn open_version(
        begin_ts: u64,
        creator_tx_id: TransactionId,
        previous_version_ptr: Option<u64>,
    ) -> AndromedaResult<Self> {
        let version = Self {
            begin_ts,
            end_ts: None,
            creator_tx_id,
            deleter_tx_id: None,
            previous_version_ptr,
            flags: 0,
        };
        version.validate()?;
        Ok(version)
    }

    pub fn visible_at(self, snapshot_ts: u64) -> bool {
        self.begin_ts <= snapshot_ts && self.end_ts.is_none_or(|end_ts| end_ts > snapshot_ts)
    }

    pub fn visible_in(self, snapshot: Snapshot) -> AndromedaResult<bool> {
        self.visible_in_snapshot(&snapshot, &TransactionStatusTable::new())
    }

    pub fn visible_in_snapshot(
        self,
        snapshot: &Snapshot,
        statuses: &TransactionStatusTable,
    ) -> AndromedaResult<bool> {
        snapshot.validate()?;
        self.validate()?;

        let creator_status =
            statuses.status_for_snapshot(self.creator_tx_id, self.begin_ts, snapshot);
        if !creator_is_visible(self.creator_tx_id, creator_status, snapshot) {
            return Ok(false);
        }

        if self.begin_ts > snapshot.timestamp
            && !snapshot.is_current_transaction(self.creator_tx_id)
        {
            return Ok(false);
        }

        let Some(end_ts) = self.end_ts else {
            return Ok(true);
        };

        let Some(deleter_tx_id) = self.deleter_tx_id else {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "closed MVCC version requires deleter transaction id",
            ));
        };

        let deleter_status = statuses.status_for_snapshot(deleter_tx_id, end_ts, snapshot);
        Ok(!delete_is_visible(
            deleter_tx_id,
            end_ts,
            deleter_status,
            snapshot,
        ))
    }

    pub const fn is_open_version(self) -> bool {
        self.end_ts.is_none()
    }

    pub const fn is_closed_version(self) -> bool {
        self.end_ts.is_some()
    }

    pub fn close_version(self, end_ts: u64, deleter_tx_id: TransactionId) -> AndromedaResult<Self> {
        let closed = Self {
            end_ts: Some(end_ts),
            deleter_tx_id: Some(deleter_tx_id),
            ..self
        };
        closed.validate()?;
        Ok(closed)
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.begin_ts == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "MVCC begin timestamp must not be zero",
            ));
        }

        if self.creator_tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "MVCC creator transaction id must not be zero",
            ));
        }

        if matches!(self.end_ts, Some(end_ts) if end_ts <= self.begin_ts) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "MVCC end timestamp must be greater than begin timestamp",
            ));
        }

        if self.end_ts.is_some() && self.deleter_tx_id.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "closed MVCC version requires deleter transaction id",
            ));
        }

        if self.end_ts.is_none() && self.deleter_tx_id.is_some() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "open MVCC version must not contain deleter transaction id",
            ));
        }

        if matches!(self.deleter_tx_id, Some(deleter_tx_id) if deleter_tx_id.get() == 0) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "MVCC deleter transaction id must not be zero",
            ));
        }

        Ok(())
    }
}

fn creator_is_visible(
    transaction_id: TransactionId,
    status: TransactionStatus,
    snapshot: &Snapshot,
) -> bool {
    match status {
        TransactionStatus::Committed => {
            !snapshot.is_transaction_active(transaction_id)
                || snapshot.is_current_transaction(transaction_id)
        }
        TransactionStatus::InFlight => snapshot.is_current_transaction(transaction_id),
        TransactionStatus::RolledBack => false,
    }
}

fn delete_is_visible(
    transaction_id: TransactionId,
    end_ts: u64,
    status: TransactionStatus,
    snapshot: &Snapshot,
) -> bool {
    match status {
        TransactionStatus::Committed => {
            end_ts <= snapshot.timestamp
                && (!snapshot.is_transaction_active(transaction_id)
                    || snapshot.is_current_transaction(transaction_id))
        }
        TransactionStatus::InFlight => snapshot.is_current_transaction(transaction_id),
        TransactionStatus::RolledBack => false,
    }
}

fn validate_transaction_id(
    transaction_id: TransactionId,
    message: &'static str,
) -> AndromedaResult<()> {
    if transaction_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvcc_visibility_uses_begin_and_end_timestamps() {
        let row = MvccRowHeader {
            begin_ts: 10,
            end_ts: Some(20),
            creator_tx_id: TransactionId::new(1),
            deleter_tx_id: Some(TransactionId::new(2)),
            previous_version_ptr: None,
            flags: 0,
        };

        assert!(!row.visible_at(9));
        assert!(row.visible_at(10));
        assert!(row.visible_at(19));
        assert!(!row.visible_at(20));
        assert!(row.validate().is_ok());
    }

    #[test]
    fn mvcc_visibility_accepts_valid_snapshot() {
        let row = MvccRowHeader::open_version(10, TransactionId::new(11), None).unwrap();

        assert!(row.visible_in(Snapshot::new(10)).unwrap());
        assert!(row.visible_in(Snapshot::new(30)).unwrap());
        assert_eq!(
            row.visible_in(Snapshot::new(0)).unwrap_err().kind(),
            AndromedaErrorKind::Transaction
        );
    }

    #[test]
    fn mvcc_open_and_closed_version_helpers_validate_shape() {
        let open = MvccRowHeader::open_version(10, TransactionId::new(12), Some(99)).unwrap();
        assert!(open.is_open_version());
        assert!(!open.is_closed_version());

        let closed = open.close_version(15, TransactionId::new(13)).unwrap();
        assert!(closed.is_closed_version());
        assert!(!closed.visible_at(15));
        assert!(closed.visible_at(14));

        assert_eq!(
            open.close_version(10, TransactionId::new(13))
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Transaction
        );
    }

    #[test]
    fn snapshot_context_normalizes_active_transactions() {
        let snapshot = Snapshot::with_context(
            50,
            CatalogVersion::new(7),
            MvccIsolationPolicy::RepeatableRead,
            Some(TransactionId::new(1)),
            [
                TransactionId::new(9),
                TransactionId::new(3),
                TransactionId::new(9),
            ],
        )
        .unwrap();

        assert_eq!(
            snapshot.active_tx_ids,
            vec![TransactionId::new(3), TransactionId::new(9)]
        );
        assert!(snapshot.is_transaction_active(TransactionId::new(3)));
    }

    #[test]
    fn status_visibility_rejects_uncommitted_and_rolled_back_creators() {
        let creator = TransactionId::new(21);
        let row = MvccRowHeader::open_version(40, creator, None).unwrap();
        let snapshot = Snapshot::with_context(
            50,
            CatalogVersion::new(2),
            MvccIsolationPolicy::RepeatableRead,
            Some(TransactionId::new(99)),
            [creator],
        )
        .unwrap();
        let mut statuses = TransactionStatusTable::new();

        statuses
            .record(creator, TransactionStatus::InFlight)
            .unwrap();
        assert!(!row.visible_in_snapshot(&snapshot, &statuses).unwrap());

        statuses
            .record(creator, TransactionStatus::RolledBack)
            .unwrap();
        assert!(!row.visible_in_snapshot(&snapshot, &statuses).unwrap());

        statuses
            .record(creator, TransactionStatus::Committed)
            .unwrap();
        assert!(!row.visible_in_snapshot(&snapshot, &statuses).unwrap());
    }

    #[test]
    fn status_visibility_ignores_uncommitted_and_rolled_back_deletes() {
        let creator = TransactionId::new(31);
        let deleter = TransactionId::new(32);
        let row = MvccRowHeader::open_version(10, creator, None)
            .unwrap()
            .close_version(40, deleter)
            .unwrap();
        let snapshot = Snapshot::with_context(
            50,
            CatalogVersion::new(4),
            MvccIsolationPolicy::ReadCommitted,
            Some(TransactionId::new(99)),
            [deleter],
        )
        .unwrap();
        let mut statuses = TransactionStatusTable::new();
        statuses
            .record(creator, TransactionStatus::Committed)
            .unwrap();

        statuses
            .record(deleter, TransactionStatus::InFlight)
            .unwrap();
        assert!(row.visible_in_snapshot(&snapshot, &statuses).unwrap());

        statuses
            .record(deleter, TransactionStatus::RolledBack)
            .unwrap();
        assert!(row.visible_in_snapshot(&snapshot, &statuses).unwrap());

        statuses
            .record(deleter, TransactionStatus::Committed)
            .unwrap();
        assert!(row.visible_in_snapshot(&snapshot, &statuses).unwrap());

        let snapshot_after_delete = Snapshot::with_context(
            50,
            CatalogVersion::new(4),
            MvccIsolationPolicy::ReadCommitted,
            Some(TransactionId::new(99)),
            Vec::<TransactionId>::new(),
        )
        .unwrap();
        assert!(
            !row.visible_in_snapshot(&snapshot_after_delete, &statuses)
                .unwrap()
        );
    }

    #[test]
    fn current_transaction_reads_own_writes_and_hides_own_deletes() {
        let tx_id = TransactionId::new(41);
        let inserted = MvccRowHeader::open_version(70, tx_id, None).unwrap();
        let deleted = inserted.close_version(80, tx_id).unwrap();
        let snapshot = Snapshot::with_context(
            60,
            CatalogVersion::new(5),
            MvccIsolationPolicy::ReadCommitted,
            Some(tx_id),
            [tx_id],
        )
        .unwrap();
        let mut statuses = TransactionStatusTable::new();
        statuses.record(tx_id, TransactionStatus::InFlight).unwrap();

        assert!(inserted.visible_in_snapshot(&snapshot, &statuses).unwrap());
        assert!(!deleted.visible_in_snapshot(&snapshot, &statuses).unwrap());
    }
}
