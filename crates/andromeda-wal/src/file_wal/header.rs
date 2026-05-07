use andromeda_core::AndromedaResult;

use crate::{Lsn, WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION};

use super::{format::file_wal_header_checksum_without_checksum, storage_error};

pub const FILE_WAL_MAGIC: u64 = 0x314c_4157_5244_4e41;
pub const FILE_WAL_HEADER_LEN: usize = 80;
pub const FILE_WAL_MONO_SEGMENT_ID: u64 = 1;

const FILE_WAL_HEADER_LEN_U32: u32 = FILE_WAL_HEADER_LEN as u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileWalHeader {
    pub magic: u64,
    pub format_version: u16,
    pub byte_order: u16,
    pub header_length: u32,
    pub segment_id: u64,
    pub first_lsn: Lsn,
    pub base_previous_lsn: Option<Lsn>,
    pub durable_lsn: Lsn,
    pub durable_bytes: u64,
    pub durable_record_count: u64,
    pub header_checksum: u64,
    pub reserved: u64,
}

impl FileWalHeader {
    pub fn new(durable_lsn: Lsn, durable_bytes: u64, durable_record_count: u64) -> Self {
        let mut header = Self {
            magic: FILE_WAL_MAGIC,
            format_version: WAL_FORMAT_VERSION,
            byte_order: WAL_BYTE_ORDER_LITTLE_ENDIAN,
            header_length: FILE_WAL_HEADER_LEN_U32,
            segment_id: FILE_WAL_MONO_SEGMENT_ID,
            first_lsn: Lsn::new(1),
            base_previous_lsn: None,
            durable_lsn,
            durable_bytes,
            durable_record_count,
            header_checksum: 0,
            reserved: 0,
        };
        header.header_checksum = file_wal_header_checksum_without_checksum(&header);
        header
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.magic != FILE_WAL_MAGIC {
            return Err(storage_error("file WAL magic mismatch"));
        }
        if self.format_version != WAL_FORMAT_VERSION {
            return Err(storage_error("unsupported file WAL format version"));
        }
        if self.byte_order != WAL_BYTE_ORDER_LITTLE_ENDIAN {
            return Err(storage_error("file WAL byte order mismatch"));
        }
        if self.header_length != FILE_WAL_HEADER_LEN_U32 {
            return Err(storage_error("file WAL header length mismatch"));
        }
        if self.segment_id != FILE_WAL_MONO_SEGMENT_ID {
            return Err(storage_error("file WAL segment id mismatch"));
        }
        if self.first_lsn != Lsn::new(1) {
            return Err(storage_error("file WAL first LSN must be 1"));
        }
        if self.base_previous_lsn.is_some() {
            return Err(storage_error(
                "file WAL mono-segment base LSN must be empty",
            ));
        }
        if self.reserved != 0 {
            return Err(storage_error("file WAL reserved bytes must be zero"));
        }
        if self.durable_lsn.is_zero() && (self.durable_bytes != 0 || self.durable_record_count != 0)
        {
            return Err(storage_error(
                "file WAL empty durable LSN must not carry durable bytes",
            ));
        }
        if !self.durable_lsn.is_zero() && self.durable_record_count == 0 {
            return Err(storage_error(
                "file WAL durable LSN requires at least one durable record",
            ));
        }
        if self.header_checksum != file_wal_header_checksum_without_checksum(self) {
            return Err(storage_error("file WAL header checksum mismatch"));
        }
        Ok(())
    }
}
