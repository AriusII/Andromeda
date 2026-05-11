use std::fmt::{Display, Formatter};

pub const PAGE_SIZE_16K: u8 = 1;
pub const PAGE_SIZE_32K: u8 = 2;
pub const PAGE_TYPE_FIXED_ROW: u8 = 1;
pub const PAGE_TYPE_HYBRID_ROW: u8 = 2;
pub const PAGE_TYPE_MANIFEST: u8 = 3;
pub const PAGE_TYPE_FREE: u8 = 4;
pub const NONE_PAGE_ID: u64 = 0;
pub const PERSISTED_HEADER_LEN: u32 = 98;
pub const PAGE_TRAILER_V0_LEN: u32 = 48;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageLayoutCodecError {
    ImageTooSmall,
    ImageLengthMismatch,
    PayloadOverlapsHeader,
    MissingLayoutMarker,
    CorruptedLayoutMarker,
    UnknownPageSize,
    UnknownPageType,
    OffsetOverflow,
    ReadOutOfBounds,
    WriteOutOfBounds,
}

impl Display for PageLayoutCodecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImageTooSmall => f.write_str("page bytes too small for layout metadata"),
            Self::ImageLengthMismatch => {
                f.write_str("persisted page size does not match image length")
            },
            Self::PayloadOverlapsHeader => {
                f.write_str("page payload offset overlaps persisted page header")
            },
            Self::MissingLayoutMarker => {
                f.write_str("persisted page layout marker is missing from non-empty page")
            },
            Self::CorruptedLayoutMarker => f.write_str("persisted page layout marker is corrupted"),
            Self::UnknownPageSize => f.write_str("unknown persisted page size"),
            Self::UnknownPageType => f.write_str("unknown persisted page type"),
            Self::OffsetOverflow => f.write_str("page layout offset overflow"),
            Self::ReadOutOfBounds => f.write_str("page layout read exceeds image"),
            Self::WriteOutOfBounds => f.write_str("page layout write exceeds image"),
        }
    }
}

impl std::error::Error for PageLayoutCodecError {}

#[deprecated(
    since = "0.0.0",
    note = "PersistedPageLayoutV1 uses an incompatible 98-byte header format. \
            Use PageCodecV1 (112-byte header) from andromeda-storage-page instead."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistedPageLayoutV1 {
    pub magic: u32,
    pub format_version: u16,
    pub page_size: u8,
    pub page_type: u8,
    pub page_id: u64,
    pub object_id: u64,
    pub allocation_id: u64,
    pub page_lsn: u64,
    pub page_epoch: u64,
    pub previous_page_id: u64,
    pub next_page_id: u64,
    pub header_len: u16,
    pub payload_offset: u32,
    pub payload_len: u32,
    pub free_start: u32,
    pub free_end: u32,
    pub free_bytes: u32,
    pub slot_count: u16,
    pub row_count: u32,
    pub flags: u16,
    pub header_crc: u32,
    pub payload_crc64: u64,
    pub page_hash: [u8; 32],
    pub torn_write_guard: u64,
}

#[allow(deprecated)]
impl PersistedPageLayoutV1 {
    pub fn encode(self, bytes: &mut [u8]) -> Result<(), PageLayoutCodecError> {
        if bytes.len() != page_size_bytes(self.page_size)? {
            return Err(PageLayoutCodecError::ImageLengthMismatch);
        }
        validate_min_layout_len(bytes)?;
        if self.payload_offset < PERSISTED_HEADER_LEN {
            return Err(PageLayoutCodecError::PayloadOverlapsHeader);
        }

        put_u32(bytes, 0, self.magic)?;
        put_u16(bytes, 4, self.format_version)?;
        put_u8(bytes, 6, self.page_size)?;
        put_u8(bytes, 7, self.page_type)?;
        put_u64(bytes, 8, self.page_id)?;
        put_u64(bytes, 16, self.object_id)?;
        put_u64(bytes, 24, self.allocation_id)?;
        put_u64(bytes, 32, self.page_lsn)?;
        put_u64(bytes, 40, self.page_epoch)?;
        put_u64(bytes, 48, self.previous_page_id)?;
        put_u64(bytes, 56, self.next_page_id)?;
        put_u16(bytes, 64, self.header_len)?;
        put_u32(bytes, 66, self.payload_offset)?;
        put_u32(bytes, 70, self.payload_len)?;
        put_u32(bytes, 74, self.free_start)?;
        put_u32(bytes, 78, self.free_end)?;
        put_u32(bytes, 82, self.free_bytes)?;
        put_u16(bytes, 86, self.slot_count)?;
        put_u32(bytes, 88, self.row_count)?;
        put_u16(bytes, 92, self.flags)?;
        put_u32(bytes, 94, self.header_crc)?;

        let trailer_offset = bytes.len() - PAGE_TRAILER_V0_LEN as usize;
        put_u64(bytes, trailer_offset, self.payload_crc64)?;
        put_slice(bytes, trailer_offset + 8, &self.page_hash)?;
        put_u64(bytes, trailer_offset + 40, self.torn_write_guard)?;
        Ok(())
    }

    pub fn decode(bytes: &[u8], expected_magic: u32) -> Result<Option<Self>, PageLayoutCodecError> {
        validate_min_layout_len(bytes)?;
        let magic = get_u32(bytes, 0)?;
        if magic == 0 {
            if bytes.iter().all(|byte| *byte == 0) {
                return Ok(None);
            }
            return Err(PageLayoutCodecError::MissingLayoutMarker);
        }
        if magic != expected_magic {
            return Err(PageLayoutCodecError::CorruptedLayoutMarker);
        }

        let page_size = get_u8(bytes, 6)?;
        if bytes.len() != page_size_bytes(page_size)? {
            return Err(PageLayoutCodecError::ImageLengthMismatch);
        }
        validate_page_type(get_u8(bytes, 7)?)?;

        let trailer_offset = bytes.len() - PAGE_TRAILER_V0_LEN as usize;
        let mut page_hash = [0; 32];
        page_hash.copy_from_slice(get_slice(bytes, trailer_offset + 8, 32)?);

        Ok(Some(Self {
            magic,
            format_version: get_u16(bytes, 4)?,
            page_size,
            page_type: get_u8(bytes, 7)?,
            page_id: get_u64(bytes, 8)?,
            object_id: get_u64(bytes, 16)?,
            allocation_id: get_u64(bytes, 24)?,
            page_lsn: get_u64(bytes, 32)?,
            page_epoch: get_u64(bytes, 40)?,
            previous_page_id: get_u64(bytes, 48)?,
            next_page_id: get_u64(bytes, 56)?,
            header_len: get_u16(bytes, 64)?,
            payload_offset: get_u32(bytes, 66)?,
            payload_len: get_u32(bytes, 70)?,
            free_start: get_u32(bytes, 74)?,
            free_end: get_u32(bytes, 78)?,
            free_bytes: get_u32(bytes, 82)?,
            slot_count: get_u16(bytes, 86)?,
            row_count: get_u32(bytes, 88)?,
            flags: get_u16(bytes, 92)?,
            header_crc: get_u32(bytes, 94)?,
            payload_crc64: get_u64(bytes, trailer_offset)?,
            page_hash,
            torn_write_guard: get_u64(bytes, trailer_offset + 40)?,
        }))
    }
}

pub const fn optional_page_id_value(value: Option<u64>) -> u64 {
    if let Some(value) = value {
        value
    } else {
        NONE_PAGE_ID
    }
}

pub const fn decode_optional_page_id(value: u64) -> Option<u64> {
    if value == NONE_PAGE_ID {
        None
    } else {
        Some(value)
    }
}

pub const fn encode_page_size_16k_or_32k(bytes: u32) -> Option<u8> {
    match bytes {
        16_384 => Some(PAGE_SIZE_16K),
        32_768 => Some(PAGE_SIZE_32K),
        _ => None,
    }
}

pub const fn page_size_bytes_const(value: u8) -> Option<usize> {
    match value {
        PAGE_SIZE_16K => Some(16 * 1024),
        PAGE_SIZE_32K => Some(32 * 1024),
        _ => None,
    }
}

fn page_size_bytes(value: u8) -> Result<usize, PageLayoutCodecError> {
    page_size_bytes_const(value).ok_or(PageLayoutCodecError::UnknownPageSize)
}

fn validate_page_type(value: u8) -> Result<(), PageLayoutCodecError> {
    match value {
        PAGE_TYPE_FIXED_ROW | PAGE_TYPE_HYBRID_ROW | PAGE_TYPE_MANIFEST | PAGE_TYPE_FREE => Ok(()),
        _ => Err(PageLayoutCodecError::UnknownPageType),
    }
}

fn validate_min_layout_len(bytes: &[u8]) -> Result<(), PageLayoutCodecError> {
    let minimum = PERSISTED_HEADER_LEN as usize + PAGE_TRAILER_V0_LEN as usize;
    if bytes.len() < minimum {
        return Err(PageLayoutCodecError::ImageTooSmall);
    }
    Ok(())
}

fn put_u8(bytes: &mut [u8], offset: usize, value: u8) -> Result<(), PageLayoutCodecError> {
    *bytes
        .get_mut(offset)
        .ok_or(PageLayoutCodecError::WriteOutOfBounds)? = value;
    Ok(())
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), PageLayoutCodecError> {
    put_slice(bytes, offset, &value.to_le_bytes())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), PageLayoutCodecError> {
    put_slice(bytes, offset, &value.to_le_bytes())
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), PageLayoutCodecError> {
    put_slice(bytes, offset, &value.to_le_bytes())
}

fn put_slice(bytes: &mut [u8], offset: usize, value: &[u8]) -> Result<(), PageLayoutCodecError> {
    let end = offset
        .checked_add(value.len())
        .ok_or(PageLayoutCodecError::OffsetOverflow)?;
    let destination = bytes
        .get_mut(offset..end)
        .ok_or(PageLayoutCodecError::WriteOutOfBounds)?;
    destination.copy_from_slice(value);
    Ok(())
}

fn get_u8(bytes: &[u8], offset: usize) -> Result<u8, PageLayoutCodecError> {
    bytes
        .get(offset)
        .copied()
        .ok_or(PageLayoutCodecError::ReadOutOfBounds)
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, PageLayoutCodecError> {
    let mut value = [0; 2];
    value.copy_from_slice(get_slice(bytes, offset, 2)?);
    Ok(u16::from_le_bytes(value))
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, PageLayoutCodecError> {
    let mut value = [0; 4];
    value.copy_from_slice(get_slice(bytes, offset, 4)?);
    Ok(u32::from_le_bytes(value))
}

fn get_u64(bytes: &[u8], offset: usize) -> Result<u64, PageLayoutCodecError> {
    let mut value = [0; 8];
    value.copy_from_slice(get_slice(bytes, offset, 8)?);
    Ok(u64::from_le_bytes(value))
}

fn get_slice(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8], PageLayoutCodecError> {
    let end = offset
        .checked_add(len)
        .ok_or(PageLayoutCodecError::OffsetOverflow)?;
    bytes
        .get(offset..end)
        .ok_or(PageLayoutCodecError::ReadOutOfBounds)
}
