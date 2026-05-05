use andromeda_core::AndromedaResult;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};

use crate::Lsn;

use super::{FILE_WAL_HEADER_LEN, FileWalHeader, io_error, storage_error};

pub(super) const FILE_WAL_DATA_OFFSET: u64 = FILE_WAL_HEADER_LEN as u64;

pub(super) fn read_file_wal_header(file: &mut File) -> AndromedaResult<FileWalHeader> {
    let mut bytes = [0; FILE_WAL_HEADER_LEN];
    file.seek(SeekFrom::Start(0))
        .map_err(|error| io_error("seek file WAL header", error))?;
    file.read_exact(&mut bytes)
        .map_err(|error| io_error("read file WAL header", error))?;
    decode_file_wal_header(&bytes)
}

pub(super) fn write_file_wal_header(
    file: &mut File,
    header: &FileWalHeader,
) -> AndromedaResult<()> {
    header.validate()?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| io_error("seek file WAL header", error))?;
    file.write_all(&encode_file_wal_header(header))
        .map_err(|error| io_error("write file WAL header", error))
}

pub(super) fn file_wal_header_checksum_without_checksum(header: &FileWalHeader) -> u64 {
    let mut bytes = Vec::with_capacity(FILE_WAL_HEADER_LEN - 8);
    push_u64(&mut bytes, header.magic);
    push_u16(&mut bytes, header.format_version);
    push_u16(&mut bytes, header.byte_order);
    push_u32(&mut bytes, header.header_length);
    push_u64(&mut bytes, header.segment_id);
    push_u64(&mut bytes, header.first_lsn.get());
    push_u64(&mut bytes, header.base_previous_lsn.map_or(0, Lsn::get));
    push_u64(&mut bytes, header.durable_lsn.get());
    push_u64(&mut bytes, header.durable_bytes);
    push_u64(&mut bytes, header.durable_record_count);
    push_u64(&mut bytes, header.reserved);
    fnv64_nonzero(&bytes)
}

pub(super) fn file_offset_for_wal_bytes(wal_bytes: u64) -> AndromedaResult<u64> {
    FILE_WAL_DATA_OFFSET
        .checked_add(wal_bytes)
        .ok_or_else(|| storage_error("file WAL offset would overflow u64"))
}

fn encode_file_wal_header(header: &FileWalHeader) -> [u8; FILE_WAL_HEADER_LEN] {
    let mut bytes = [0; FILE_WAL_HEADER_LEN];
    write_u64(&mut bytes, 0, header.magic);
    write_u16(&mut bytes, 8, header.format_version);
    write_u16(&mut bytes, 10, header.byte_order);
    write_u32(&mut bytes, 12, header.header_length);
    write_u64(&mut bytes, 16, header.segment_id);
    write_u64(&mut bytes, 24, header.first_lsn.get());
    write_u64(&mut bytes, 32, header.base_previous_lsn.map_or(0, Lsn::get));
    write_u64(&mut bytes, 40, header.durable_lsn.get());
    write_u64(&mut bytes, 48, header.durable_bytes);
    write_u64(&mut bytes, 56, header.durable_record_count);
    write_u64(&mut bytes, 64, header.header_checksum);
    write_u64(&mut bytes, 72, header.reserved);
    bytes
}

fn decode_file_wal_header(bytes: &[u8; FILE_WAL_HEADER_LEN]) -> AndromedaResult<FileWalHeader> {
    let base_previous_lsn = read_u64(bytes, 32);
    let header = FileWalHeader {
        magic: read_u64(bytes, 0),
        format_version: read_u16(bytes, 8),
        byte_order: read_u16(bytes, 10),
        header_length: read_u32(bytes, 12),
        segment_id: read_u64(bytes, 16),
        first_lsn: Lsn::new(read_u64(bytes, 24)),
        base_previous_lsn: (base_previous_lsn != 0).then_some(Lsn::new(base_previous_lsn)),
        durable_lsn: Lsn::new(read_u64(bytes, 40)),
        durable_bytes: read_u64(bytes, 48),
        durable_record_count: read_u64(bytes, 56),
        header_checksum: read_u64(bytes, 64),
        reserved: read_u64(bytes, 72),
    };
    header.validate()?;
    Ok(header)
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("file WAL u16 slice"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("file WAL u32 slice"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("file WAL u64 slice"),
    )
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
