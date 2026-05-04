use andromeda_core::{AndromedaResult, TransactionId};

use crate::{WalRecord, WalRecordHeader};

use super::{
    binary::{push_u16, push_u64},
    frame::{decode_frame_header, WalFrameHeader},
    storage_error, WAL_RECORD_HEADER_LEN,
};

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
    push_u64(&mut encoded, header.previous_lsn.map_or(0, crate::Lsn::get));
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
