use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use crate::{wal_record_kind_from_tag, wal_record_kind_tag, Lsn, WalRecord, WalRecordHeader};

pub const WAL_FORMAT_VERSION: u16 = 0;
pub const WAL_RECORD_MAGIC: u64 = 0x414e_4452_4f57_414c;
pub const WAL_RECORD_HEADER_LEN: usize = 72;

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
pub struct WalScanResult {
    pub records: Vec<WalRecord>,
    pub valid_bytes: usize,
    pub last_valid_lsn: Option<Lsn>,
    pub stopped: Option<WalScanStop>,
}

impl WalScanResult {
    pub fn is_complete(&self) -> bool {
        self.stopped.is_none()
    }
}

pub fn encode_wal_record(record: &WalRecord) -> AndromedaResult<Vec<u8>> {
    let header = WalFrameHeader::from_record(record)?;
    let capacity = usize::try_from(header.total_length)
        .map_err(|_| storage_error("WAL frame total length does not fit usize"))?;
    let mut encoded = Vec::with_capacity(capacity);
    push_u64(&mut encoded, header.magic);
    push_u16(&mut encoded, header.format_version);
    push_u16(&mut encoded, header.header_length);
    push_u64(&mut encoded, header.total_length);
    push_u16(&mut encoded, header.kind_tag);
    push_u16(&mut encoded, header.flags);
    push_u64(&mut encoded, header.lsn.get());
    push_u64(&mut encoded, header.previous_lsn.map_or(0, Lsn::get));
    push_u64(
        &mut encoded,
        header.transaction_id.map_or(0, TransactionId::get),
    );
    push_u64(&mut encoded, header.payload_length);
    push_u64(&mut encoded, header.record_checksum);
    push_u64(&mut encoded, header.header_checksum);
    encoded.extend_from_slice(record.payload());
    Ok(encoded)
}

pub fn decode_wal_record_frame(buffer: &[u8]) -> AndromedaResult<Option<(WalRecord, usize)>> {
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
    let record = WalRecord::new(
        WalRecordHeader {
            kind: header.kind()?,
            lsn: header.lsn,
            previous_lsn: header.previous_lsn,
            transaction_id: header.transaction_id,
            payload_length: header.payload_length,
            checksum: header.record_checksum,
        },
        payload,
    )?;

    Ok(Some((record, total_length)))
}

pub fn scan_wal_records(buffer: &[u8]) -> WalScanResult {
    scan_wal_records_from(buffer, Lsn::new(1), None)
}

pub fn scan_wal_records_from(
    buffer: &[u8],
    first_lsn: Lsn,
    base_previous_lsn: Option<Lsn>,
) -> WalScanResult {
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

        let header = match decode_frame_header(&buffer[offset..offset + WAL_RECORD_HEADER_LEN]) {
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

        let record = match decode_wal_record_frame(&buffer[offset..offset + total_length]) {
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

        previous_lsn_for_chain = Some(record.header.lsn);
        last_valid_lsn = Some(record.header.lsn);
        next_lsn = record.header.lsn.checked_next();
        offset += total_length;
        records.push(record);
    }

    WalScanResult {
        records,
        valid_bytes: offset,
        last_valid_lsn,
        stopped: None,
    }
}

pub fn decode_frame_header(buffer: &[u8]) -> AndromedaResult<WalFrameHeader> {
    if buffer.len() < WAL_RECORD_HEADER_LEN {
        return Err(storage_error("truncated WAL frame header"));
    }

    let magic = read_u64(buffer, 0);
    let format_version = read_u16(buffer, 8);
    let header_length = read_u16(buffer, 10);
    let total_length = read_u64(buffer, 12);
    let kind_tag = read_u16(buffer, 20);
    let flags = read_u16(buffer, 22);
    let lsn = Lsn::new(read_u64(buffer, 24));
    let previous_lsn_value = read_u64(buffer, 32);
    let transaction_id_value = read_u64(buffer, 40);
    let payload_length = read_u64(buffer, 48);
    let record_checksum = read_u64(buffer, 56);
    let header_checksum = read_u64(buffer, 64);

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

fn scan_result(
    records: Vec<WalRecord>,
    offset: usize,
    last_valid_lsn: Option<Lsn>,
    reason: WalScanStopReason,
) -> WalScanResult {
    WalScanResult {
        records,
        valid_bytes: offset,
        last_valid_lsn,
        stopped: Some(WalScanStop { offset, reason }),
    }
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

fn header_checksum_without_checksum(header: &WalFrameHeader) -> u64 {
    let mut bytes = Vec::with_capacity(WAL_RECORD_HEADER_LEN - 8);
    push_u64(&mut bytes, header.magic);
    push_u16(&mut bytes, header.format_version);
    push_u16(&mut bytes, header.header_length);
    push_u64(&mut bytes, header.total_length);
    push_u16(&mut bytes, header.kind_tag);
    push_u16(&mut bytes, header.flags);
    push_u64(&mut bytes, header.lsn.get());
    push_u64(&mut bytes, header.previous_lsn.map_or(0, Lsn::get));
    push_u64(
        &mut bytes,
        header.transaction_id.map_or(0, TransactionId::get),
    );
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

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("u16 frame slice"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("u64 frame slice"),
    )
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WalRecordKind;

    fn tx_record(lsn: u64, previous_lsn: Option<u64>, payload: &[u8]) -> WalRecord {
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(lsn),
            previous_lsn.map(Lsn::new),
            Some(TransactionId::new(7)),
            payload,
        )
            .unwrap()
    }

    fn encode_records(records: &[WalRecord]) -> Vec<u8> {
        let mut encoded = Vec::new();
        for record in records {
            encoded.extend(encode_wal_record(record).unwrap());
        }
        encoded
    }

    #[test]
    fn valid_record_scan_returns_all_records() {
        let records = vec![tx_record(1, None, b"a"), tx_record(2, Some(1), b"bb")];
        let scan = scan_wal_records(&encode_records(&records));

        assert!(scan.is_complete());
        assert_eq!(scan.records, records);
        assert_eq!(scan.last_valid_lsn, Some(Lsn::new(2)));
    }

    #[test]
    fn scan_stops_at_truncated_tail_and_keeps_prefix() {
        let records = vec![tx_record(1, None, b"a"), tx_record(2, Some(1), b"bb")];
        let mut encoded = encode_records(&records);
        encoded.truncate(encoded.len() - 1);

        let scan = scan_wal_records(&encoded);

        assert_eq!(scan.records, vec![records[0].clone()]);
        assert_eq!(
            scan.stopped.unwrap().reason,
            WalScanStopReason::TruncatedRecord
        );
    }

    #[test]
    fn scan_stops_at_corrupted_tail_and_keeps_prefix() {
        let records = vec![tx_record(1, None, b"a"), tx_record(2, Some(1), b"bb")];
        let mut encoded = encode_records(&records);
        let tail_payload_byte = encoded.len() - 1;
        encoded[tail_payload_byte] ^= 0x55;

        let scan = scan_wal_records(&encoded);

        assert_eq!(scan.records, vec![records[0].clone()]);
        assert_eq!(
            scan.stopped.unwrap().reason,
            WalScanStopReason::CorruptRecord
        );
    }

    #[test]
    fn scan_rejects_skipped_lsn() {
        let records = vec![tx_record(1, None, b"a"), tx_record(3, Some(1), b"bb")];
        let scan = scan_wal_records(&encode_records(&records));

        assert_eq!(scan.records.len(), 1);
        assert_eq!(scan.stopped.unwrap().reason, WalScanStopReason::LsnGap);
    }

    #[test]
    fn scan_rejects_duplicate_lsn() {
        let records = vec![tx_record(1, None, b"a"), tx_record(1, None, b"bb")];
        let scan = scan_wal_records(&encode_records(&records));

        assert_eq!(scan.records.len(), 1);
        assert_eq!(
            scan.stopped.unwrap().reason,
            WalScanStopReason::DuplicateOrReorderedLsn
        );
    }

    #[test]
    fn scan_rejects_previous_lsn_mismatch() {
        let records = vec![tx_record(1, None, b"a"), tx_record(2, None, b"bb")];
        let scan = scan_wal_records(&encode_records(&records));

        assert_eq!(scan.records.len(), 1);
        assert_eq!(
            scan.stopped.unwrap().reason,
            WalScanStopReason::PreviousLsnMismatch
        );
    }
}
