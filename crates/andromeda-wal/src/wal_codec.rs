//! Typed WAL frame codec facade.
//!
//! `andromeda-wal-codec` owns the raw byte format and scan loop. This module
//! adapts that raw codec to `Lsn`, `TransactionId`, and `WalRecord` so existing
//! WAL owner APIs remain typed and recovery-safe.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_wal_codec::{
    WalCodecFrameHeader, WalCodecRecordFrame, encode_wal_record_frame, encoded_wal_record_frame_len,
};

use crate::{
    Lsn, WalRecord, WalRecordHeader, WalRecordKind, wal_record_kind_from_tag, wal_record_kind_tag,
};

pub use andromeda_wal_codec::{
    WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION, WAL_FORMAT_VERSION_V1, WAL_RECORD_HEADER_LEN,
    WAL_RECORD_MAGIC, WalScanStop, WalScanStopReason,
};

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
        let raw = raw_record_frame_from_record(record)?;
        Self::from_raw(WalCodecFrameHeader::from_record_frame(&raw)?)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.to_raw().validate()?;
        self.kind()?;
        Ok(())
    }

    pub fn kind(&self) -> AndromedaResult<WalRecordKind> {
        wal_record_kind_from_tag(u64::from(self.kind_tag))
            .ok_or_else(|| storage_error("unknown WAL record kind tag"))
    }

    fn from_raw(raw: WalCodecFrameHeader) -> AndromedaResult<Self> {
        raw.validate()?;
        let header = Self {
            magic: raw.magic,
            format_version: raw.format_version,
            header_length: raw.header_length,
            total_length: raw.total_length,
            kind_tag: raw.kind_tag,
            flags: raw.flags,
            lsn: Lsn::new(raw.lsn),
            previous_lsn: raw.previous_lsn.map(Lsn::new),
            transaction_id: raw.transaction_id.map(TransactionId::new),
            payload_length: raw.payload_length,
            record_checksum: raw.record_checksum,
            header_checksum: raw.header_checksum,
        };
        header.validate()?;
        Ok(header)
    }

    fn to_raw(self) -> WalCodecFrameHeader {
        WalCodecFrameHeader {
            magic: self.magic,
            format_version: self.format_version,
            header_length: self.header_length,
            total_length: self.total_length,
            kind_tag: self.kind_tag,
            flags: self.flags,
            lsn: self.lsn.get(),
            previous_lsn: self.previous_lsn.map(Lsn::get),
            transaction_id: self.transaction_id.map(TransactionId::get),
            payload_length: self.payload_length,
            record_checksum: self.record_checksum,
            header_checksum: self.header_checksum,
        }
    }
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
    record.validate()?;
    encode_wal_record_frame(&raw_record_frame_from_record(record)?)
}

pub fn encoded_wal_record_len(record: &WalRecord) -> AndromedaResult<u64> {
    record.validate()?;
    encoded_wal_record_frame_len(record.header.payload_length)
}

pub fn decode_frame_header(buffer: &[u8]) -> AndromedaResult<WalFrameHeader> {
    WalFrameHeader::from_raw(andromeda_wal_codec::decode_frame_header(buffer)?)
}

pub fn decode_wal_record_frame(buffer: &[u8]) -> AndromedaResult<Option<(WalRecord, usize)>> {
    let Some((raw, consumed)) = andromeda_wal_codec::decode_wal_record_frame(buffer)? else {
        return Ok(None);
    };
    let record = record_from_raw(raw)?;
    Ok(Some((record, consumed)))
}

pub fn scan_wal_records(buffer: &[u8]) -> WalScanResult {
    scan_wal_records_from(buffer, Lsn::new(1), None)
}

pub fn scan_wal_records_from(
    buffer: &[u8],
    first_lsn: Lsn,
    base_previous_lsn: Option<Lsn>,
) -> WalScanResult {
    let raw = andromeda_wal_codec::scan_wal_record_frames_from_with(
        buffer,
        first_lsn.get(),
        base_previous_lsn.map(Lsn::get),
        |header| WalFrameHeader::from_raw(*header).map(|_| ()),
        decode_wal_record_frame,
    );
    WalScanResult {
        records: raw.records,
        valid_bytes: raw.valid_bytes,
        last_valid_lsn: raw.last_valid_lsn.map(Lsn::new),
        stopped: raw.stopped,
    }
}

fn raw_record_frame_from_record(record: &WalRecord) -> AndromedaResult<WalCodecRecordFrame> {
    let kind_tag = u16::try_from(wal_record_kind_tag(record.header.kind))
        .map_err(|_| storage_error("WAL record kind tag does not fit frame"))?;
    Ok(WalCodecRecordFrame {
        kind_tag,
        lsn: record.header.lsn.get(),
        previous_lsn: record.header.previous_lsn.map(Lsn::get),
        transaction_id: record.header.transaction_id.map(TransactionId::get),
        payload: record.payload.clone(),
        record_checksum: record.header.checksum,
    })
}

fn record_from_raw(raw: WalCodecRecordFrame) -> AndromedaResult<WalRecord> {
    let kind = wal_record_kind_from_tag(u64::from(raw.kind_tag))
        .ok_or_else(|| storage_error("unknown WAL record kind tag"))?;
    WalRecord::new(
        WalRecordHeader {
            kind,
            lsn: Lsn::new(raw.lsn),
            previous_lsn: raw.previous_lsn.map(Lsn::new),
            transaction_id: raw.transaction_id.map(TransactionId::new),
            payload_length: raw.payload.len() as u64,
            checksum: raw.record_checksum,
        },
        raw.payload,
    )
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
