use andromeda_core::{AndromedaResult, TransactionId};

use crate::{Lsn, WalRecord, wal_record_kind_from_tag, wal_record_kind_tag};

use super::{
    WAL_FORMAT_VERSION, WAL_RECORD_HEADER_LEN, WAL_RECORD_MAGIC,
    binary::{read_u16, read_u64},
    checksum::header_checksum_without_checksum,
    storage_error,
};

const FLAG_HAS_PREVIOUS_LSN: u16 = 0x0001;
const FLAG_HAS_TRANSACTION_ID: u16 = 0x0002;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalFrameHeader {
    pub magic: u64,
    pub format_version: u16,
    pub header_length: u16,
    pub total_length: u64,
    pub kind_tag: u16,
    pub flags: u16,
    pub lsn: Lsn,
    pub previous_lsn: Option<Lsn>,
    pub transaction_id: Option<TransactionId>,
    pub payload_length: u64,
    pub record_checksum: u64,
    pub header_checksum: u64,
}

impl WalFrameHeader {
    pub fn from_record(record: &WalRecord) -> AndromedaResult<Self> {
        record.validate()?;
        let payload_length = record.payload.len() as u64;
        let total_length = (WAL_RECORD_HEADER_LEN as u64)
            .checked_add(payload_length)
            .ok_or_else(|| storage_error("WAL frame total length would overflow u64"))?;
        let kind_tag = u16::try_from(wal_record_kind_tag(record.header.kind))
            .map_err(|_| storage_error("WAL record kind tag does not fit frame"))?;
        let mut header = Self {
            magic: WAL_RECORD_MAGIC,
            format_version: WAL_FORMAT_VERSION,
            header_length: WAL_RECORD_HEADER_LEN as u16,
            total_length,
            kind_tag,
            flags: flags_for(record.header.previous_lsn, record.header.transaction_id),
            lsn: record.header.lsn,
            previous_lsn: record.header.previous_lsn,
            transaction_id: record.header.transaction_id,
            payload_length,
            record_checksum: record.header.checksum,
            header_checksum: 0,
        };
        header.header_checksum = header_checksum_without_checksum(&header);
        Ok(header)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.magic != WAL_RECORD_MAGIC {
            return Err(storage_error("WAL frame magic mismatch"));
        }
        if self.format_version != WAL_FORMAT_VERSION {
            return Err(storage_error("unsupported WAL frame format version"));
        }
        if usize::from(self.header_length) != WAL_RECORD_HEADER_LEN {
            return Err(storage_error("WAL frame header length mismatch"));
        }
        if self.total_length < u64::from(self.header_length) {
            return Err(storage_error(
                "WAL frame total length is smaller than header",
            ));
        }
        if self.payload_length != self.total_length - u64::from(self.header_length) {
            return Err(storage_error("WAL frame payload length mismatch"));
        }
        if self.kind().is_err() {
            return Err(storage_error("unknown WAL record kind tag"));
        }
        if self.flags & !(FLAG_HAS_PREVIOUS_LSN | FLAG_HAS_TRANSACTION_ID) != 0 {
            return Err(storage_error("unknown WAL frame flags"));
        }
        if self.previous_lsn.is_some() != (self.flags & FLAG_HAS_PREVIOUS_LSN != 0) {
            return Err(storage_error("WAL frame previous LSN flag mismatch"));
        }
        if self.transaction_id.is_some() != (self.flags & FLAG_HAS_TRANSACTION_ID != 0) {
            return Err(storage_error("WAL frame transaction id flag mismatch"));
        }
        if self.header_checksum != header_checksum_without_checksum(self) {
            return Err(storage_error("WAL frame header checksum mismatch"));
        }
        Ok(())
    }

    pub fn kind(&self) -> AndromedaResult<crate::WalRecordKind> {
        wal_record_kind_from_tag(u64::from(self.kind_tag))
            .ok_or_else(|| storage_error("unknown WAL record kind tag"))
    }
}

pub fn decode_frame_header(buffer: &[u8]) -> AndromedaResult<WalFrameHeader> {
    if buffer.len() < WAL_RECORD_HEADER_LEN {
        return Err(storage_error("truncated WAL frame header"));
    }

    let magic = read_u64(buffer, 0)?;
    let format_version = read_u16(buffer, 8)?;
    let header_length = read_u16(buffer, 10)?;
    let total_length = read_u64(buffer, 12)?;
    let kind_tag = read_u16(buffer, 20)?;
    let flags = read_u16(buffer, 22)?;
    let lsn = Lsn::new(read_u64(buffer, 24)?);
    let previous_lsn_value = read_u64(buffer, 32)?;
    let transaction_id_value = read_u64(buffer, 40)?;
    let payload_length = read_u64(buffer, 48)?;
    let record_checksum = read_u64(buffer, 56)?;
    let header_checksum = read_u64(buffer, 64)?;

    if flags & FLAG_HAS_PREVIOUS_LSN == 0 && previous_lsn_value != 0 {
        return Err(storage_error("WAL frame has unflagged previous LSN bytes"));
    }
    if flags & FLAG_HAS_TRANSACTION_ID == 0 && transaction_id_value != 0 {
        return Err(storage_error(
            "WAL frame has unflagged transaction id bytes",
        ));
    }

    let header = WalFrameHeader {
        magic,
        format_version,
        header_length,
        total_length,
        kind_tag,
        flags,
        lsn,
        previous_lsn: if flags & FLAG_HAS_PREVIOUS_LSN != 0 {
            Some(Lsn::new(previous_lsn_value))
        } else {
            None
        },
        transaction_id: if flags & FLAG_HAS_TRANSACTION_ID != 0 {
            Some(TransactionId::new(transaction_id_value))
        } else {
            None
        },
        payload_length,
        record_checksum,
        header_checksum,
    };
    header.validate()?;
    Ok(header)
}

fn flags_for(previous_lsn: Option<Lsn>, transaction_id: Option<TransactionId>) -> u16 {
    let mut flags = 0;
    if previous_lsn.is_some() {
        flags |= FLAG_HAS_PREVIOUS_LSN;
    }
    if transaction_id.is_some() {
        flags |= FLAG_HAS_TRANSACTION_ID;
    }
    flags
}
