use super::super::{
    digest::{crc32_iso_hdlc, sha256},
    types::{SegmentIndexEntryV0, SegmentIndexHeaderV0, SegmentIndexTrailerV0, SegmentIndexV0},
};
use super::{
    ENTRY_CRC_OFFSET, HEADER_CRC_OFFSET, TRAILER_CRC_OFFSET,
    encode::{encode_entry, encode_file, encode_header, encode_trailer},
    policy::TrailerHashMode,
};

pub(super) fn header_crc32(header: &SegmentIndexHeaderV0) -> u32 {
    crc32_iso_hdlc(
        &encode_header(header),
        HEADER_CRC_OFFSET..HEADER_CRC_OFFSET + 4,
    )
}

pub(super) fn entry_crc32(entry: &SegmentIndexEntryV0) -> u32 {
    crc32_iso_hdlc(&encode_entry(entry), ENTRY_CRC_OFFSET..ENTRY_CRC_OFFSET + 4)
}

pub(super) fn trailer_crc32(trailer: &SegmentIndexTrailerV0) -> u32 {
    crc32_iso_hdlc(
        &encode_trailer(trailer, TrailerHashMode::Stored),
        TRAILER_CRC_OFFSET..TRAILER_CRC_OFFSET + 4,
    )
}

pub(super) fn file_sha256(index: &SegmentIndexV0, trailer: &SegmentIndexTrailerV0) -> [u8; 32] {
    let mut input = index.clone();
    input.trailer = trailer.clone();
    sha256(&encode_file(&input, TrailerHashMode::AcyclicFileHashInput))
}

pub(super) fn root_hash(header: &SegmentIndexHeaderV0, file_sha256: [u8; 32]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(32 + 8 + 8 + 32 + 32);
    bytes.extend_from_slice(&header.parent_manifest_hash);
    bytes.extend_from_slice(&header.segment_index_id.to_le_bytes());
    bytes.extend_from_slice(&header.snapshot_id.to_le_bytes());
    bytes.extend_from_slice(&header.entry_table_sha256);
    bytes.extend_from_slice(&file_sha256);
    sha256(&bytes)
}
