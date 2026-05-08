#![forbid(unsafe_code)]
#![doc = r#"
Explicit WAL frame codec for Andromeda.

This crate owns byte-format constants, little-endian field encoding, WAL frame
header validation, raw frame encode/decode, and the bounded scan loop. It does
not own WAL record semantics, replay policy, fsync behavior, or storage
recovery. Owner crates provide typed record validation through narrow wrapper
functions.
"#]

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalCodecRecordFrame {
    pub kind_tag: u16,
    pub lsn: u64,
    pub previous_lsn: Option<u64>,
    pub transaction_id: Option<u64>,
    pub payload: Vec<u8>,
    pub record_checksum: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalScanStopReason {
    TruncatedHeader,
    TruncatedRecord,
    CorruptHeader,
    CorruptRecord,
    LsnGap,
    DuplicateOrReorderedLsn,
    PreviousLsnMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalScanStop {
    pub offset: usize,
    pub reason: WalScanStopReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalCodecScanResult<T> {
    pub records: Vec<T>,
    pub valid_bytes: usize,
    pub last_valid_lsn: Option<u64>,
    pub stopped: Option<WalScanStop>,
}

impl<T> WalCodecScanResult<T> {
    pub fn is_complete(&self) -> bool {
        self.stopped.is_none()
    }
}

pub fn encode_wal_record_frame(frame: &WalCodecRecordFrame) -> AndromedaResult<Vec<u8>> {
    let header = WalCodecFrameHeader::from_record_frame(frame)?;
    let capacity = usize::try_from(header.total_length)
        .map_err(|_| storage_error("WAL frame total length does not fit usize"))?;
    let mut encoded = Vec::with_capacity(capacity);
    push_u64(&mut encoded, header.magic);
    push_u16(&mut encoded, header.format_version);
    push_u16(&mut encoded, header.header_length);
    push_u64(&mut encoded, header.total_length);
    push_u16(&mut encoded, header.kind_tag);
    push_u16(&mut encoded, header.flags);
    push_u64(&mut encoded, header.lsn);
    push_u64(&mut encoded, header.previous_lsn.unwrap_or(0));
    push_u64(&mut encoded, header.transaction_id.unwrap_or(0));
    push_u64(&mut encoded, header.payload_length);
    push_u64(&mut encoded, header.record_checksum);
    push_u64(&mut encoded, header.header_checksum);
    encoded.extend_from_slice(&frame.payload);
    Ok(encoded)
}

pub fn encoded_wal_record_frame_len(payload_length: u64) -> AndromedaResult<u64> {
    let total_length = (WAL_RECORD_HEADER_LEN as u64)
        .checked_add(payload_length)
        .ok_or_else(|| storage_error("WAL frame total length would overflow u64"))?;
    usize::try_from(total_length)
        .map_err(|_| storage_error("WAL frame total length does not fit usize"))?;
    Ok(total_length)
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

pub fn decode_wal_record_frame(
    buffer: &[u8],
) -> AndromedaResult<Option<(WalCodecRecordFrame, usize)>> {
    if buffer.is_empty() {
        return Ok(None);
    }
    if buffer.len() < WAL_RECORD_HEADER_LEN {
        return Err(storage_error("truncated WAL frame header"));
    }

    let header = decode_frame_header(&buffer[..WAL_RECORD_HEADER_LEN])?;
    let total_length = usize::try_from(header.total_length)
        .map_err(|_| storage_error("WAL frame total length does not fit usize"))?;
    if buffer.len() < total_length {
        return Err(storage_error("truncated WAL frame payload"));
    }

    let payload = buffer[WAL_RECORD_HEADER_LEN..total_length].to_vec();
    let frame = WalCodecRecordFrame {
        kind_tag: header.kind_tag,
        lsn: header.lsn,
        previous_lsn: header.previous_lsn,
        transaction_id: header.transaction_id,
        payload,
        record_checksum: header.record_checksum,
    };

    Ok(Some((frame, total_length)))
}

pub fn scan_wal_record_frames(buffer: &[u8]) -> WalCodecScanResult<WalCodecRecordFrame> {
    scan_wal_record_frames_from(buffer, 1, None)
}

pub fn scan_wal_record_frames_from(
    buffer: &[u8],
    first_lsn: u64,
    base_previous_lsn: Option<u64>,
) -> WalCodecScanResult<WalCodecRecordFrame> {
    scan_wal_record_frames_from_with(
        buffer,
        first_lsn,
        base_previous_lsn,
        |_| Ok(()),
        decode_wal_record_frame,
    )
}

pub fn scan_wal_record_frames_from_with<T, ValidateHeader, DecodeFrame>(
    buffer: &[u8],
    first_lsn: u64,
    base_previous_lsn: Option<u64>,
    mut validate_header: ValidateHeader,
    mut decode_frame: DecodeFrame,
) -> WalCodecScanResult<T>
where
    ValidateHeader: FnMut(&WalCodecFrameHeader) -> AndromedaResult<()>,
    DecodeFrame: FnMut(&[u8]) -> AndromedaResult<Option<(T, usize)>>,
{
    let mut records = Vec::new();
    let mut offset = 0;
    let mut previous_lsn_for_chain = base_previous_lsn;
    let mut last_valid_lsn = None;
    let mut next_lsn = Some(first_lsn);

    while offset < buffer.len() {
        if buffer.len() - offset < WAL_RECORD_HEADER_LEN {
            return scan_result(
                records,
                offset,
                last_valid_lsn,
                WalScanStopReason::TruncatedHeader,
            );
        }

        let header = match decode_frame_header(&buffer[offset..offset + WAL_RECORD_HEADER_LEN])
            .and_then(|header| {
                validate_header(&header)?;
                Ok(header)
            }) {
            Ok(header) => header,
            Err(_) => {
                return scan_result(
                    records,
                    offset,
                    last_valid_lsn,
                    WalScanStopReason::CorruptHeader,
                );
            }
        };

        let total_length = match usize::try_from(header.total_length) {
            Ok(total_length) => total_length,
            Err(_) => {
                return scan_result(
                    records,
                    offset,
                    last_valid_lsn,
                    WalScanStopReason::CorruptHeader,
                );
            }
        };
        if buffer.len() - offset < total_length {
            return scan_result(
                records,
                offset,
                last_valid_lsn,
                WalScanStopReason::TruncatedRecord,
            );
        }

        let Some(expected_lsn) = next_lsn else {
            return scan_result(
                records,
                offset,
                last_valid_lsn,
                WalScanStopReason::DuplicateOrReorderedLsn,
            );
        };

        if header.lsn < expected_lsn {
            return scan_result(
                records,
                offset,
                last_valid_lsn,
                WalScanStopReason::DuplicateOrReorderedLsn,
            );
        }
        if header.lsn > expected_lsn {
            return scan_result(records, offset, last_valid_lsn, WalScanStopReason::LsnGap);
        }
        if previous_lsn_for_chain != header.previous_lsn {
            return scan_result(
                records,
                offset,
                last_valid_lsn,
                WalScanStopReason::PreviousLsnMismatch,
            );
        }

        let record = match decode_frame(&buffer[offset..offset + total_length]) {
            Ok(Some((record, consumed))) if consumed == total_length => record,
            _ => {
                return scan_result(
                    records,
                    offset,
                    last_valid_lsn,
                    WalScanStopReason::CorruptRecord,
                );
            }
        };

        previous_lsn_for_chain = Some(header.lsn);
        last_valid_lsn = Some(header.lsn);
        next_lsn = header.lsn.checked_add(1);
        offset += total_length;
        records.push(record);
    }

    WalCodecScanResult {
        records,
        valid_bytes: offset,
        last_valid_lsn,
        stopped: None,
    }
}

fn scan_result<T>(
    records: Vec<T>,
    offset: usize,
    last_valid_lsn: Option<u64>,
    reason: WalScanStopReason,
) -> WalCodecScanResult<T> {
    WalCodecScanResult {
        records,
        valid_bytes: offset,
        last_valid_lsn,
        stopped: Some(WalScanStop { offset, reason }),
    }
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

fn header_checksum_without_checksum(header: &WalCodecFrameHeader) -> u64 {
    let mut bytes = Vec::with_capacity(WAL_RECORD_HEADER_LEN - 8);
    push_u64(&mut bytes, header.magic);
    push_u16(&mut bytes, header.format_version);
    push_u16(&mut bytes, header.header_length);
    push_u64(&mut bytes, header.total_length);
    push_u16(&mut bytes, header.kind_tag);
    push_u16(&mut bytes, header.flags);
    push_u64(&mut bytes, header.lsn);
    push_u64(&mut bytes, header.previous_lsn.unwrap_or(0));
    push_u64(&mut bytes, header.transaction_id.unwrap_or(0));
    push_u64(&mut bytes, header.payload_length);
    push_u64(&mut bytes, header.record_checksum);
    fnv64_nonzero(&bytes)
}

fn fnv64_nonzero(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut state = FNV_OFFSET;
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    if state == 0 { 1 } else { state }
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> AndromedaResult<u16> {
    Ok(u16::from_le_bytes(read_array(
        bytes,
        offset,
        "WAL frame u16 field out of bounds",
    )?))
}

fn read_u64(bytes: &[u8], offset: usize) -> AndromedaResult<u64> {
    Ok(u64::from_le_bytes(read_array(
        bytes,
        offset,
        "WAL frame u64 field out of bounds",
    )?))
}

fn read_array<const N: usize>(
    bytes: &[u8],
    offset: usize,
    error_message: &'static str,
) -> AndromedaResult<[u8; N]> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| storage_error(error_message))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| storage_error(error_message))?
        .try_into()
        .map_err(|_| storage_error(error_message))
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
