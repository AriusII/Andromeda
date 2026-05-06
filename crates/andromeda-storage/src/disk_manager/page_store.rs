use std::path::Path;

use andromeda_core::AndromedaResult;

use crate::{
    AllocationId, ExtentDescriptor, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageImage,
    PageLayoutContract, PageSize, PageStore, PageTrailer, PageType, integrity_trailer_for_payload,
    validate_payload_integrity,
};

use super::{DiskManager, DiskManagerError, FileDiskManager, PageIntegrityMode};

/// Adapter to present FileDiskManager as a page-store façade.
pub struct DiskPageStore {
    manager: FileDiskManager,
    page_size: PageSize,
}

impl DiskPageStore {
    pub fn new(file_path: impl AsRef<Path>, temp_dir: impl AsRef<Path>) -> AndromedaResult<Self> {
        let manager = FileDiskManager::open(file_path, temp_dir)?;
        Ok(Self {
            manager,
            page_size: PageSize::KiB16,
        })
    }

    pub fn new_with_integrity(
        file_path: impl AsRef<Path>,
        temp_dir: impl AsRef<Path>,
        page_integrity_mode: PageIntegrityMode,
    ) -> AndromedaResult<Self> {
        let manager =
            FileDiskManager::open_with_integrity(file_path, temp_dir, page_integrity_mode)?;
        Ok(Self {
            manager,
            page_size: PageSize::KiB16,
        })
    }

    pub fn disk_manager_mut(&mut self) -> &mut FileDiskManager {
        &mut self.manager
    }

    pub fn allocate_extent(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()> {
        self.page_size = descriptor.page_size;
        self.manager.allocate_extent(descriptor)
    }

    pub fn register_extent(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()> {
        self.page_size = descriptor.page_size;
        self.manager.register_extent(descriptor)
    }

    pub fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>> {
        <Self as PageStore>::read_page(self, page_id)
    }

    fn validate_page_id(page_id: PageId) -> AndromedaResult<()> {
        if page_id.is_zero() {
            return Err(storage_error("disk page store page id must not be zero"));
        }
        Ok(())
    }

    fn validate_image_for_store(&self, image: &PageImage) -> AndromedaResult<PageLayoutContract> {
        if image.page_size() != self.page_size {
            return Err(storage_error(
                "page image size does not match disk page store size",
            ));
        }
        let layout_contract = image
            .layout_contract()
            .ok_or_else(|| storage_error("page image must include a validated layout contract"))?;
        layout_contract.validate()?;
        Self::validate_page_id(layout_contract.header.page_id)?;
        Ok(layout_contract)
    }

    fn validate_wal_before_page_flush(image: &PageImage, durable_lsn: Lsn) -> AndromedaResult<()> {
        let page_lsn = image
            .page_lsn()
            .ok_or_else(|| storage_error("page image must expose page LSN through its layout"))?;
        if durable_lsn < page_lsn {
            return Err(DiskManagerError::WalFenceViolation {
                page_lsn: page_lsn.get(),
                durable_lsn: durable_lsn.get(),
            }
            .into());
        }
        Ok(())
    }

    fn image_with_persisted_layout(image: PageImage) -> AndromedaResult<PageImage> {
        let mut layout_contract = image
            .layout_contract()
            .ok_or_else(|| storage_error("page image must include a validated layout contract"))?;
        let mut bytes = image.into_bytes();
        refresh_layout_integrity(&mut layout_contract, &bytes)?;
        encode_layout_contract(layout_contract, &mut bytes)?;
        PageImage::with_layout(layout_contract, bytes)
    }
}

impl PageStore for DiskPageStore {
    fn page_size(&self) -> PageSize {
        self.page_size
    }

    fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>> {
        Self::validate_page_id(page_id)?;
        let Some(image) = self.manager.read_page(page_id)? else {
            return Ok(None);
        };
        let Some(layout_contract) = decode_layout_contract(image.as_bytes())? else {
            return Ok(None);
        };
        if layout_contract.header.page_id != page_id {
            return Err(storage_error(
                "persisted page layout has mismatched page id",
            ));
        }
        PageImage::with_layout(layout_contract, image.into_bytes()).map(Some)
    }

    fn write_page(&mut self, image: PageImage, durable_lsn: Lsn) -> AndromedaResult<()> {
        let layout_contract = self.validate_image_for_store(&image)?;
        Self::validate_wal_before_page_flush(&image, durable_lsn)?;
        if self
            .manager
            .extent_for_page(layout_contract.header.page_id)?
            .is_none()
        {
            return Err(storage_error(
                "disk page store write requires allocated extent",
            ));
        }
        self.manager
            .write_page(Self::image_with_persisted_layout(image)?, durable_lsn)
    }

    fn allocate_page(
        &mut self,
        layout_contract: PageLayoutContract,
        durable_lsn: Lsn,
    ) -> AndromedaResult<PageImage> {
        if layout_contract.header.page_size != self.page_size {
            return Err(storage_error(
                "allocated page size does not match disk page store size",
            ));
        }
        layout_contract.validate()?;
        Self::validate_page_id(layout_contract.header.page_id)?;
        let image = PageImage::zeroed_with_layout(layout_contract)?;
        Self::validate_wal_before_page_flush(&image, durable_lsn)?;
        if self
            .manager
            .extent_for_page(layout_contract.header.page_id)?
            .is_none()
        {
            return Err(storage_error(
                "disk page store allocation requires an allocated extent",
            ));
        }
        let durable_image = Self::image_with_persisted_layout(image)?;
        self.manager
            .write_page(durable_image.clone(), durable_lsn)?;
        Ok(durable_image)
    }
}

const PAGE_SIZE_16K: u8 = 1;
const PAGE_SIZE_32K: u8 = 2;
const PAGE_TYPE_FIXED_ROW: u8 = 1;
const PAGE_TYPE_HYBRID_ROW: u8 = 2;
const PAGE_TYPE_MANIFEST: u8 = 3;
const PAGE_TYPE_FREE: u8 = 4;
const NONE_PAGE_ID: u64 = 0;
const PERSISTED_HEADER_LEN: u32 = 98;

fn encode_layout_contract(layout: PageLayoutContract, bytes: &mut [u8]) -> AndromedaResult<()> {
    if bytes.len() != layout.header.page_size.bytes_usize() {
        return Err(storage_error("page bytes do not match layout page size"));
    }
    if bytes.len() < PageHeader::MIN_HEADER_LEN_V0 as usize + PageTrailer::V0_LEN as usize {
        return Err(storage_error("page bytes too small for layout metadata"));
    }
    if layout.header.payload_offset < PERSISTED_HEADER_LEN {
        return Err(storage_error(
            "page payload offset overlaps persisted page header",
        ));
    }

    put_u32(bytes, 0, layout.header.magic)?;
    put_u16(bytes, 4, layout.header.format_version)?;
    put_u8(bytes, 6, encode_page_size(layout.header.page_size))?;
    put_u8(bytes, 7, encode_page_type(layout.header.page_type))?;
    put_u64(bytes, 8, layout.header.page_id.get())?;
    put_u64(bytes, 16, layout.header.object_id.get())?;
    put_u64(bytes, 24, layout.header.allocation_id.get())?;
    put_u64(bytes, 32, layout.header.page_lsn.get())?;
    put_u64(bytes, 40, layout.header.page_epoch)?;
    put_u64(
        bytes,
        48,
        layout
            .header
            .previous_page_id
            .map(PageId::get)
            .unwrap_or(NONE_PAGE_ID),
    )?;
    put_u64(
        bytes,
        56,
        layout
            .header
            .next_page_id
            .map(PageId::get)
            .unwrap_or(NONE_PAGE_ID),
    )?;
    put_u16(bytes, 64, layout.header.header_len)?;
    put_u32(bytes, 66, layout.header.payload_offset)?;
    put_u32(bytes, 70, layout.header.payload_len)?;
    put_u32(bytes, 74, layout.header.free_start)?;
    put_u32(bytes, 78, layout.header.free_end)?;
    put_u32(bytes, 82, layout.header.free_bytes)?;
    put_u16(bytes, 86, layout.header.slot_count)?;
    put_u32(bytes, 88, layout.header.row_count)?;
    put_u16(bytes, 92, layout.header.flags.bits())?;
    put_u32(bytes, 94, layout.header.header_crc)?;

    let trailer_offset = bytes.len() - PageTrailer::V0_LEN as usize;
    put_u64(bytes, trailer_offset, layout.trailer.payload_crc64)?;
    put_slice(bytes, trailer_offset + 8, &layout.trailer.page_hash)?;
    put_u64(bytes, trailer_offset + 40, layout.trailer.torn_write_guard)?;
    Ok(())
}

fn decode_layout_contract(bytes: &[u8]) -> AndromedaResult<Option<PageLayoutContract>> {
    if bytes.len() < PageHeader::MIN_HEADER_LEN_V0 as usize + PageTrailer::V0_LEN as usize {
        return Err(storage_error("page bytes too small for layout metadata"));
    }
    let magic = get_u32(bytes, 0)?;
    if magic == 0 {
        if bytes.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }
        return Err(storage_error(
            "persisted page layout marker is missing from non-empty page",
        ));
    }
    if magic != PageHeader::MAGIC {
        return Err(storage_error("persisted page layout marker is corrupted"));
    }

    let page_size = decode_page_size(get_u8(bytes, 6)?)?;
    if bytes.len() != page_size.bytes_usize() {
        return Err(storage_error(
            "persisted page size does not match image length",
        ));
    }

    let previous_page_id = optional_page_id(get_u64(bytes, 48)?);
    let next_page_id = optional_page_id(get_u64(bytes, 56)?);
    let trailer_offset = bytes.len() - PageTrailer::V0_LEN as usize;
    let mut page_hash = [0; 32];
    page_hash.copy_from_slice(get_slice(bytes, trailer_offset + 8, 32)?);

    let layout_contract = PageLayoutContract {
        header: PageHeader {
            magic,
            format_version: get_u16(bytes, 4)?,
            page_size,
            page_type: decode_page_type(get_u8(bytes, 7)?)?,
            page_id: PageId::new(get_u64(bytes, 8)?),
            object_id: ObjectId::new(get_u64(bytes, 16)?),
            allocation_id: AllocationId::new(get_u64(bytes, 24)?),
            page_lsn: Lsn::new(get_u64(bytes, 32)?),
            page_epoch: get_u64(bytes, 40)?,
            previous_page_id,
            next_page_id,
            header_len: get_u16(bytes, 64)?,
            payload_offset: get_u32(bytes, 66)?,
            payload_len: get_u32(bytes, 70)?,
            free_start: get_u32(bytes, 74)?,
            free_end: get_u32(bytes, 78)?,
            free_bytes: get_u32(bytes, 82)?,
            slot_count: get_u16(bytes, 86)?,
            row_count: get_u32(bytes, 88)?,
            flags: PageFlags::new(get_u16(bytes, 92)?),
            header_crc: get_u32(bytes, 94)?,
        },
        trailer: PageTrailer {
            payload_crc64: get_u64(bytes, trailer_offset)?,
            page_hash,
            torn_write_guard: get_u64(bytes, trailer_offset + 40)?,
        },
    };
    layout_contract.validate()?;
    validate_payload_integrity(
        &layout_contract.header,
        payload_slice(&layout_contract.header, bytes)?,
        &layout_contract.trailer,
    )?;
    Ok(Some(layout_contract))
}

fn refresh_layout_integrity(layout: &mut PageLayoutContract, bytes: &[u8]) -> AndromedaResult<()> {
    layout.trailer =
        integrity_trailer_for_payload(&layout.header, payload_slice(&layout.header, bytes)?);
    layout.validate()
}

fn payload_slice<'a>(header: &PageHeader, bytes: &'a [u8]) -> AndromedaResult<&'a [u8]> {
    let start = usize::try_from(header.payload_offset)
        .map_err(|_| storage_error("page payload offset does not fit usize"))?;
    let len = usize::try_from(header.payload_len)
        .map_err(|_| storage_error("page payload length does not fit usize"))?;
    get_slice(bytes, start, len)
}

fn encode_page_size(page_size: PageSize) -> u8 {
    match page_size {
        PageSize::KiB16 => PAGE_SIZE_16K,
        PageSize::KiB32 => PAGE_SIZE_32K,
    }
}

fn decode_page_size(value: u8) -> AndromedaResult<PageSize> {
    match value {
        PAGE_SIZE_16K => Ok(PageSize::KiB16),
        PAGE_SIZE_32K => Ok(PageSize::KiB32),
        _ => Err(storage_error("unknown persisted page size")),
    }
}

fn encode_page_type(page_type: PageType) -> u8 {
    match page_type {
        PageType::FixedRow => PAGE_TYPE_FIXED_ROW,
        PageType::HybridRow => PAGE_TYPE_HYBRID_ROW,
        PageType::Manifest => PAGE_TYPE_MANIFEST,
        PageType::Free => PAGE_TYPE_FREE,
    }
}

fn decode_page_type(value: u8) -> AndromedaResult<PageType> {
    match value {
        PAGE_TYPE_FIXED_ROW => Ok(PageType::FixedRow),
        PAGE_TYPE_HYBRID_ROW => Ok(PageType::HybridRow),
        PAGE_TYPE_MANIFEST => Ok(PageType::Manifest),
        PAGE_TYPE_FREE => Ok(PageType::Free),
        _ => Err(storage_error("unknown persisted page type")),
    }
}

fn optional_page_id(value: u64) -> Option<PageId> {
    if value == NONE_PAGE_ID {
        None
    } else {
        Some(PageId::new(value))
    }
}

fn put_u8(bytes: &mut [u8], offset: usize, value: u8) -> AndromedaResult<()> {
    *bytes
        .get_mut(offset)
        .ok_or_else(|| storage_error("page layout write exceeds image"))? = value;
    Ok(())
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) -> AndromedaResult<()> {
    put_slice(bytes, offset, &value.to_le_bytes())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> AndromedaResult<()> {
    put_slice(bytes, offset, &value.to_le_bytes())
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) -> AndromedaResult<()> {
    put_slice(bytes, offset, &value.to_le_bytes())
}

fn put_slice(bytes: &mut [u8], offset: usize, value: &[u8]) -> AndromedaResult<()> {
    let end = offset
        .checked_add(value.len())
        .ok_or_else(|| storage_error("page layout offset overflow"))?;
    let destination = bytes
        .get_mut(offset..end)
        .ok_or_else(|| storage_error("page layout write exceeds image"))?;
    destination.copy_from_slice(value);
    Ok(())
}

fn get_u8(bytes: &[u8], offset: usize) -> AndromedaResult<u8> {
    bytes
        .get(offset)
        .copied()
        .ok_or_else(|| storage_error("page layout read exceeds image"))
}

fn get_u16(bytes: &[u8], offset: usize) -> AndromedaResult<u16> {
    let mut value = [0; 2];
    value.copy_from_slice(get_slice(bytes, offset, 2)?);
    Ok(u16::from_le_bytes(value))
}

fn get_u32(bytes: &[u8], offset: usize) -> AndromedaResult<u32> {
    let mut value = [0; 4];
    value.copy_from_slice(get_slice(bytes, offset, 4)?);
    Ok(u32::from_le_bytes(value))
}

fn get_u64(bytes: &[u8], offset: usize) -> AndromedaResult<u64> {
    let mut value = [0; 8];
    value.copy_from_slice(get_slice(bytes, offset, 8)?);
    Ok(u64::from_le_bytes(value))
}

fn get_slice(bytes: &[u8], offset: usize, len: usize) -> AndromedaResult<&[u8]> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| storage_error("page layout offset overflow"))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| storage_error("page layout read exceeds image"))
}

fn storage_error(message: impl Into<String>) -> andromeda_core::AndromedaError {
    DiskManagerError::PageLayoutInvalid {
        reason: message.into(),
    }
    .into()
}
