use andromeda_error::AndromedaResult;

use crate::{
    binary::{push_u16, push_u64},
    header::{WAL_RECORD_HEADER_LEN, WalCodecFrameHeader, decode_frame_header},
    storage_error,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalCodecRecordFrame {
    pub kind_tag: u16,
    pub lsn: u64,
    pub previous_lsn: Option<u64>,
    pub transaction_id: Option<u64>,
    pub payload: Vec<u8>,
    pub record_checksum: u64,
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
