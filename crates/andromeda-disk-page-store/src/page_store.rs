use std::path::Path;

use andromeda_error::AndromedaResult;
use andromeda_segment::ExtentDescriptor;
use andromeda_storage_page::{
    AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageImage, PageLayoutContract,
    PageSize, PageStore, PageTrailer, PageType, integrity_trailer_for_payload,
    validate_payload_integrity,
};

use crate::{
    PAGE_SIZE_16K, PAGE_SIZE_32K, PAGE_TYPE_FIXED_ROW, PAGE_TYPE_FREE, PAGE_TYPE_HYBRID_ROW,
    PAGE_TYPE_MANIFEST, PageFlushDurabilityBoundary, PageFlushDurabilityError,
    PageLayoutCodecError, PersistedPageLayoutV1, decode_optional_page_id,
    encode_page_size_16k_or_32k, optional_page_id_value,
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
        PageFlushDurabilityBoundary::new(page_lsn, durable_lsn)
            .validate()
            .map_err(map_page_flush_error)
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

fn encode_layout_contract(layout: PageLayoutContract, bytes: &mut [u8]) -> AndromedaResult<()> {
    PersistedPageLayoutV1 {
        magic: layout.header.magic,
        format_version: layout.header.format_version,
        page_size: encode_page_size(layout.header.page_size)?,
        page_type: encode_page_type(layout.header.page_type),
        page_id: layout.header.page_id.get(),
        object_id: layout.header.object_id.get(),
        allocation_id: layout.header.allocation_id.get(),
        page_lsn: layout.header.page_lsn.get(),
        page_epoch: layout.header.page_epoch,
        previous_page_id: optional_page_id_value(layout.header.previous_page_id.map(PageId::get)),
        next_page_id: optional_page_id_value(layout.header.next_page_id.map(PageId::get)),
        header_len: layout.header.header_len,
        payload_offset: layout.header.payload_offset,
        payload_len: layout.header.payload_len,
        free_start: layout.header.free_start,
        free_end: layout.header.free_end,
        free_bytes: layout.header.free_bytes,
        slot_count: layout.header.slot_count,
        row_count: layout.header.row_count,
        flags: layout.header.flags.bits(),
        header_crc: layout.header.header_crc,
        payload_crc64: layout.trailer.payload_crc64,
        page_hash: layout.trailer.page_hash,
        torn_write_guard: layout.trailer.torn_write_guard,
    }
    .encode(bytes)
    .map_err(map_layout_codec_error)
}

fn decode_layout_contract(bytes: &[u8]) -> AndromedaResult<Option<PageLayoutContract>> {
    let Some(layout) =
        PersistedPageLayoutV1::decode(bytes, PageHeader::MAGIC).map_err(map_layout_codec_error)?
    else {
        return Ok(None);
    };

    let page_size = decode_page_size(layout.page_size)?;
    let previous_page_id = decode_optional_page_id(layout.previous_page_id).map(PageId::new);
    let next_page_id = decode_optional_page_id(layout.next_page_id).map(PageId::new);

    let layout_contract = PageLayoutContract {
        header: PageHeader {
            magic: layout.magic,
            format_version: layout.format_version,
            page_size,
            page_type: decode_page_type(layout.page_type)?,
            page_id: PageId::new(layout.page_id),
            object_id: ObjectId::new(layout.object_id),
            allocation_id: AllocationId::new(layout.allocation_id),
            page_lsn: Lsn::new(layout.page_lsn),
            page_epoch: layout.page_epoch,
            previous_page_id,
            next_page_id,
            header_len: layout.header_len,
            payload_offset: layout.payload_offset,
            payload_len: layout.payload_len,
            free_start: layout.free_start,
            free_end: layout.free_end,
            free_bytes: layout.free_bytes,
            slot_count: layout.slot_count,
            row_count: layout.row_count,
            flags: PageFlags::new(layout.flags),
            header_crc: layout.header_crc,
        },
        trailer: PageTrailer {
            payload_crc64: layout.payload_crc64,
            page_hash: layout.page_hash,
            torn_write_guard: layout.torn_write_guard,
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
    slice_at(bytes, start, len)
}

fn encode_page_size(page_size: PageSize) -> AndromedaResult<u8> {
    encode_page_size_16k_or_32k(page_size.bytes())
        .ok_or_else(|| storage_error("unknown persisted page size"))
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

fn slice_at(bytes: &[u8], offset: usize, len: usize) -> AndromedaResult<&[u8]> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| storage_error("page layout offset overflow"))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| storage_error("page layout read exceeds image"))
}

fn map_page_flush_error(error: PageFlushDurabilityError) -> andromeda_error::AndromedaError {
    match error {
        PageFlushDurabilityError::MissingPageLsn => DiskManagerError::PageLayoutInvalid {
            reason: "page flush requires a nonzero page LSN".to_string(),
        }
        .into(),
        PageFlushDurabilityError::WalFenceViolation {
            page_lsn,
            durable_lsn,
        } => DiskManagerError::WalFenceViolation {
            page_lsn,
            durable_lsn,
        }
        .into(),
    }
}

fn map_layout_codec_error(error: PageLayoutCodecError) -> andromeda_error::AndromedaError {
    storage_error(error.to_string())
}

fn storage_error(message: impl Into<String>) -> andromeda_error::AndromedaError {
    DiskManagerError::PageLayoutInvalid {
        reason: message.into(),
    }
    .into()
}
