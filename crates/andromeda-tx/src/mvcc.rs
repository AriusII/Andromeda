use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub timestamp: u64,
}

impl Snapshot {
    pub const fn new(timestamp: u64) -> Self {
        Self { timestamp }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.timestamp == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "snapshot timestamp must not be zero",
            ));
        }

        Ok(())
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
        snapshot.validate()?;
        self.validate()?;
        Ok(self.visible_at(snapshot.timestamp))
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
}
