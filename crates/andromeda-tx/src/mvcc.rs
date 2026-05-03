use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub timestamp: u64,
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
    pub fn visible_at(self, snapshot_ts: u64) -> bool {
        self.begin_ts <= snapshot_ts && self.end_ts.is_none_or(|end_ts| end_ts > snapshot_ts)
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if matches!(self.end_ts, Some(end_ts) if end_ts <= self.begin_ts) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "MVCC end timestamp must be greater than begin timestamp",
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
}
