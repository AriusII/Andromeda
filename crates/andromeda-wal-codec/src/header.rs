use andromeda_error::AndromedaResult;

use crate::{
    binary::{read_u16, read_u64},
    checksum::header_checksum_without_checksum,
    frame::{WalCodecRecordFrame, encoded_wal_record_frame_len},
    storage_error,
};

pub const WAL_FORMAT_VERSION_V1: u16 = 1;
pub const WAL_FORMAT_VERSION: u16 = WAL_FORMAT_VERSION_V1;
pub const WAL_BYTE_ORDER_LITTLE_ENDIAN: u16 = 0x0102;
pub const WAL_RECORD_MAGIC: u64 = 0x414e_4452_4f57_414c;
pub const WAL_RECORD_HEADER_LEN: usize = 72;

const FLAG_HAS_PREVIOUS_LSN: u16 = 0x0001;
const FLAG_HAS_TRANSACTION_ID: u16 = 0x0002;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalCodecFrameHeader {
    pub magic: u64,
    pub format_version: u16,
    pub header_length: u16,
    pub total_length: u64,
    pub kind_tag: u16,
    pub flags: u16,
    pub lsn: u64,
    pub previous_lsn: Option<u64>,
    pub transaction_id: Option<u64>,
    pub payload_length: u64,
    pub record_checksum: u64,
    pub header_checksum: u64,
}

impl WalCodecFrameHeader {
    pub fn from_record_frame(frame: &WalCodecRecordFrame) -> AndromedaResult<Self> {
        let payload_length = frame.payload.len() as u64;
        let total_length = encoded_wal_record_frame_len(payload_length)?;
        let mut header = Self {
            magic: WAL_RECORD_MAGIC,
            format_version: WAL_FORMAT_VERSION,
            header_length: WAL_RECORD_HEADER_LEN as u16,
            total_length,
            kind_tag: frame.kind_tag,
            flags: flags_for(frame.previous_lsn, frame.transaction_id),
            lsn: frame.lsn,
            previous_lsn: frame.previous_lsn,
            transaction_id: frame.transaction_id,
            payload_length,
            record_checksum: frame.record_checksum,
            header_checksum: 0,
        };
        header.header_checksum = header_checksum_without_checksum(&header);
        header.validate()?;
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
}

pub fn decode_frame_header(buffer: &[u8]) -> AndromedaResult<WalCodecFrameHeader> {
    if buffer.len() < WAL_RECORD_HEADER_LEN {
        return Err(storage_error("truncated WAL frame header"));
    }

    let magic = read_u64(buffer, 0)?;
    let format_version = read_u16(buffer, 8)?;
    let header_length = read_u16(buffer, 10)?;
    let total_length = read_u64(buffer, 12)?;
    let kind_tag = read_u16(buffer, 20)?;
    let flags = read_u16(buffer, 22)?;
    let lsn = read_u64(buffer, 24)?;
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

    let header = WalCodecFrameHeader {
        magic,
        format_version,
        header_length,
        total_length,
        kind_tag,
        flags,
        lsn,
        previous_lsn: if flags & FLAG_HAS_PREVIOUS_LSN != 0 {
            Some(previous_lsn_value)
        } else {
            None
        },
        transaction_id: if flags & FLAG_HAS_TRANSACTION_ID != 0 {
            Some(transaction_id_value)
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

fn flags_for(previous_lsn: Option<u64>, transaction_id: Option<u64>) -> u16 {
    let mut flags = 0;
    if previous_lsn.is_some() {
        flags |= FLAG_HAS_PREVIOUS_LSN;
    }
    if transaction_id.is_some() {
        flags |= FLAG_HAS_TRANSACTION_ID;
    }
    flags
}
