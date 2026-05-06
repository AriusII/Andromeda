//! WAL record types, structure, and validation.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use crate::Lsn;

/// WAL record type taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalRecordKind {
    TxBegin,
    TxCommit,
    TxRollback,
    PageAllocate,
    PageFormat,
    RowInsert,
    RowUpdate,
    RowDelete,
    IndexInsert,
    IndexDelete,
    MvccVersionCreate,
    MvccVersionClose,
    MapDeltaAppend,
    CheckpointBegin,
    CheckpointEnd,
    SnapshotBegin,
    SnapshotEnd,
    ManifestSwitch,
    CatalogChangeBegin,
    CatalogChangeApply,
    CatalogChangeCommit,
    SecurityAuditAppend,
    /// B-Tree record insert mutation. Durable replay is fail-stop until promoted.
    BTreeInsert,
    /// B-Tree record delete mutation. Durable replay is fail-stop until promoted.
    BTreeDelete,
    /// B-Tree node split operation. Durable replay is fail-stop until promoted.
    BTreeSplit,
    /// B-Tree node merge operation. Durable replay is fail-stop until promoted.
    BTreeMerge,
}

impl WalRecordKind {
    pub const fn is_transaction_boundary(self) -> bool {
        matches!(self, Self::TxBegin | Self::TxCommit | Self::TxRollback)
    }

    pub const fn requires_transaction_id(self) -> bool {
        matches!(
            self,
            Self::TxBegin
                | Self::TxCommit
                | Self::TxRollback
                | Self::RowInsert
                | Self::RowUpdate
                | Self::RowDelete
                | Self::IndexInsert
                | Self::IndexDelete
                | Self::MvccVersionCreate
                | Self::MvccVersionClose
                | Self::CatalogChangeBegin
                | Self::CatalogChangeApply
                | Self::CatalogChangeCommit
                | Self::BTreeInsert
                | Self::BTreeDelete
                | Self::BTreeSplit
                | Self::BTreeMerge
        )
    }

    pub const fn is_redo_relevant(self) -> bool {
        matches!(
            self,
            Self::PageAllocate
                | Self::PageFormat
                | Self::RowInsert
                | Self::RowUpdate
                | Self::RowDelete
                | Self::IndexInsert
                | Self::IndexDelete
                | Self::MvccVersionCreate
                | Self::MvccVersionClose
                | Self::MapDeltaAppend
                | Self::ManifestSwitch
                | Self::CatalogChangeApply
                | Self::CatalogChangeCommit
                | Self::SecurityAuditAppend
        )
    }
}

/// WAL record header with metadata and checksums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalRecordHeader {
    pub kind: WalRecordKind,
    pub lsn: Lsn,
    pub previous_lsn: Option<Lsn>,
    pub transaction_id: Option<TransactionId>,
    pub payload_length: u64,
    pub checksum: u64,
}

impl WalRecordHeader {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.lsn == Lsn::default() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record LSN must not be zero",
            ));
        }

        if self.kind.requires_transaction_id() && self.transaction_id.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record kind requires a transaction id",
            ));
        }

        if self.checksum == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record checksum must not be zero",
            ));
        }

        Ok(())
    }
}

/// Complete WAL record with header and payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalRecord {
    pub header: WalRecordHeader,
    pub payload: Vec<u8>,
}

impl WalRecord {
    pub fn new(header: WalRecordHeader, payload: Vec<u8>) -> AndromedaResult<Self> {
        let record = Self { header, payload };
        record.validate()?;
        Ok(record)
    }

    pub fn from_parts(
        kind: WalRecordKind,
        lsn: Lsn,
        previous_lsn: Option<Lsn>,
        transaction_id: Option<TransactionId>,
        payload: impl Into<Vec<u8>>,
    ) -> AndromedaResult<Self> {
        let payload = payload.into();
        let payload_length = payload.len() as u64;
        let checksum = wal_record_checksum(kind, lsn, previous_lsn, transaction_id, &payload);
        Self::new(
            WalRecordHeader {
                kind,
                lsn,
                previous_lsn,
                transaction_id,
                payload_length,
                checksum,
            },
            payload,
        )
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.header.validate()?;

        if self.header.payload_length != self.payload.len() as u64 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record payload length mismatch",
            ));
        }

        if matches!(self.header.previous_lsn, Some(previous_lsn) if previous_lsn >= self.header.lsn)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record previous LSN must precede record LSN",
            ));
        }

        let expected_checksum = wal_record_checksum(
            self.header.kind,
            self.header.lsn,
            self.header.previous_lsn,
            self.header.transaction_id,
            &self.payload,
        );
        if self.header.checksum != expected_checksum {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record checksum mismatch",
            ));
        }

        Ok(())
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub const fn transaction_id(&self) -> Option<TransactionId> {
        self.header.transaction_id
    }

    pub fn is_for_transaction(&self, transaction_id: TransactionId) -> bool {
        matches!(self.header.transaction_id, Some(id) if id.get() == transaction_id.get())
    }

    pub const fn is_transaction_terminal(&self) -> bool {
        matches!(
            self.header.kind,
            WalRecordKind::TxCommit | WalRecordKind::TxRollback
        )
    }
}

/// Compute FNV-1a 64-bit hash for WAL record integrity.
pub fn wal_record_checksum(
    kind: WalRecordKind,
    lsn: Lsn,
    previous_lsn: Option<Lsn>,
    transaction_id: Option<TransactionId>,
    payload: &[u8],
) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    fn fold_u64(state: &mut u64, value: u64) {
        for byte in value.to_le_bytes() {
            *state ^= u64::from(byte);
            *state = state.wrapping_mul(FNV_PRIME);
        }
    }

    let mut state = FNV_OFFSET;
    fold_u64(&mut state, wal_record_kind_tag(kind));
    fold_u64(&mut state, lsn.get());
    fold_u64(&mut state, previous_lsn.map_or(0, Lsn::get));
    fold_u64(&mut state, transaction_id.map_or(0, TransactionId::get));
    fold_u64(&mut state, payload.len() as u64);

    for byte in payload {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }

    if state == 0 { 1 } else { state }
}

/// Map WalRecordKind to numeric tag for checksum computation.
pub fn wal_record_kind_tag(kind: WalRecordKind) -> u64 {
    match kind {
        WalRecordKind::TxBegin => 1,
        WalRecordKind::TxCommit => 2,
        WalRecordKind::TxRollback => 3,
        WalRecordKind::PageAllocate => 4,
        WalRecordKind::PageFormat => 5,
        WalRecordKind::RowInsert => 6,
        WalRecordKind::RowUpdate => 7,
        WalRecordKind::RowDelete => 8,
        WalRecordKind::IndexInsert => 9,
        WalRecordKind::IndexDelete => 10,
        WalRecordKind::MvccVersionCreate => 11,
        WalRecordKind::MvccVersionClose => 12,
        WalRecordKind::MapDeltaAppend => 13,
        WalRecordKind::CheckpointBegin => 14,
        WalRecordKind::CheckpointEnd => 15,
        WalRecordKind::SnapshotBegin => 16,
        WalRecordKind::SnapshotEnd => 17,
        WalRecordKind::ManifestSwitch => 18,
        WalRecordKind::CatalogChangeBegin => 19,
        WalRecordKind::CatalogChangeApply => 20,
        WalRecordKind::CatalogChangeCommit => 21,
        WalRecordKind::SecurityAuditAppend => 22,
        WalRecordKind::BTreeInsert => 23,
        WalRecordKind::BTreeDelete => 24,
        WalRecordKind::BTreeSplit => 25,
        WalRecordKind::BTreeMerge => 26,
    }
}

/// Reconstruct WalRecordKind from numeric tag.
pub fn wal_record_kind_from_tag(tag: u64) -> Option<WalRecordKind> {
    match tag {
        1 => Some(WalRecordKind::TxBegin),
        2 => Some(WalRecordKind::TxCommit),
        3 => Some(WalRecordKind::TxRollback),
        4 => Some(WalRecordKind::PageAllocate),
        5 => Some(WalRecordKind::PageFormat),
        6 => Some(WalRecordKind::RowInsert),
        7 => Some(WalRecordKind::RowUpdate),
        8 => Some(WalRecordKind::RowDelete),
        9 => Some(WalRecordKind::IndexInsert),
        10 => Some(WalRecordKind::IndexDelete),
        11 => Some(WalRecordKind::MvccVersionCreate),
        12 => Some(WalRecordKind::MvccVersionClose),
        13 => Some(WalRecordKind::MapDeltaAppend),
        14 => Some(WalRecordKind::CheckpointBegin),
        15 => Some(WalRecordKind::CheckpointEnd),
        16 => Some(WalRecordKind::SnapshotBegin),
        17 => Some(WalRecordKind::SnapshotEnd),
        18 => Some(WalRecordKind::ManifestSwitch),
        19 => Some(WalRecordKind::CatalogChangeBegin),
        20 => Some(WalRecordKind::CatalogChangeApply),
        21 => Some(WalRecordKind::CatalogChangeCommit),
        22 => Some(WalRecordKind::SecurityAuditAppend),
        23 => Some(WalRecordKind::BTreeInsert),
        24 => Some(WalRecordKind::BTreeDelete),
        25 => Some(WalRecordKind::BTreeSplit),
        26 => Some(WalRecordKind::BTreeMerge),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_taxonomy_marks_transaction_boundaries() {
        assert!(WalRecordKind::TxBegin.is_transaction_boundary());
        assert!(WalRecordKind::TxCommit.is_transaction_boundary());
        assert!(WalRecordKind::TxRollback.is_transaction_boundary());
        assert!(!WalRecordKind::RowInsert.is_transaction_boundary());
        assert!(!WalRecordKind::BTreeInsert.is_transaction_boundary());
    }

    #[test]
    fn wal_record_requires_transaction_id_when_needed() {
        let record = WalRecordHeader {
            kind: WalRecordKind::RowInsert,
            lsn: Lsn::new(1),
            previous_lsn: None,
            transaction_id: None,
            payload_length: 8,
            checksum: 99,
        };

        assert_eq!(
            record.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
