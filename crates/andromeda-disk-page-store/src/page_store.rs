use std::path::Path;

use andromeda_error::AndromedaResult;
use andromeda_segment::ExtentDescriptor;
use andromeda_storage_page::{
    Lsn, PAGE_CODEC_V1_HEADER_LEN, PAGE_CODEC_V1_TRAILER_LEN, PageCodecV1, PageHeader, PageId,
    PageImage, PageLayoutContract, PageSize, PageStore, integrity_trailer_for_payload,
    validate_payload_integrity,
};

use crate::{PageFlushDurabilityBoundary, PageFlushDurabilityError};

use super::{DiskManager, DiskManagerError, FileDiskManager, PageIntegrityMode};

/// Adapter to present FileDiskManager as a page-store façade.
///
/// All pages are encoded and decoded using the canonical PageCodecV1 format
/// (112-byte header + variable payload + free space + 48-byte trailer within
/// a fixed-size page buffer of exactly `page_size` bytes).
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

    /// Encode a `PageImage` with the canonical PageCodecV1 format.
    ///
    /// Writes the 112-byte PageCodecV1 header into `bytes[0..112]` and the
    /// 48-byte PageCodecV1 trailer into `bytes[page_size-48..page_size]`.
    /// The payload and free-space region between them is left unchanged.
    fn image_with_v1_codec(image: PageImage) -> AndromedaResult<PageImage> {
        let mut layout_contract = image
            .layout_contract()
            .ok_or_else(|| storage_error("page image must include a validated layout contract"))?;
        let mut bytes = image.into_bytes();
        refresh_layout_integrity(&mut layout_contract, &bytes)?;
        // Encode header using PageCodecV1 (112-byte header format).
        let header_bytes = PageCodecV1::encode_header(&layout_contract.header)
            .map_err(|e| storage_error(e.message()))?;
        bytes[..PAGE_CODEC_V1_HEADER_LEN].copy_from_slice(&header_bytes);
        // Encode trailer using PageCodecV1 (48-byte trailer format).
        let trailer_bytes = PageCodecV1::encode_trailer(&layout_contract.trailer)
            .map_err(|e| storage_error(e.message()))?;
        let trailer_start = bytes.len() - PAGE_CODEC_V1_TRAILER_LEN;
        bytes[trailer_start..].copy_from_slice(&trailer_bytes);
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
            .write_page(Self::image_with_v1_codec(image)?, durable_lsn)
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
        let durable_image = Self::image_with_v1_codec(image)?;
        self.manager
            .write_page(durable_image.clone(), durable_lsn)?;
        Ok(durable_image)
    }
}

/// Decode a PageLayoutContract from raw page buffer bytes using PageCodecV1.
///
/// Returns `None` for all-zero (unallocated) pages. Returns an error for
/// structurally invalid or integrity-failing pages.
fn decode_layout_contract(bytes: &[u8]) -> AndromedaResult<Option<PageLayoutContract>> {
    if bytes.len() < PAGE_CODEC_V1_HEADER_LEN + PAGE_CODEC_V1_TRAILER_LEN {
        return Err(storage_error(
            "page image is too small for PageCodecV1 header and trailer",
        ));
    }
    // All-zero buffer → unallocated page.
    if bytes.iter().all(|&b| b == 0) {
        return Ok(None);
    }
    let header = PageCodecV1::decode_header(&bytes[..PAGE_CODEC_V1_HEADER_LEN])
        .map_err(|e| storage_error(e.message()))?;
    let trailer_start = bytes.len() - PAGE_CODEC_V1_TRAILER_LEN;
    let trailer = PageCodecV1::decode_trailer(&bytes[trailer_start..])
        .map_err(|e| storage_error(e.message()))?;
    let layout_contract = PageLayoutContract { header, trailer };
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

fn storage_error(message: impl Into<String>) -> andromeda_error::AndromedaError {
    DiskManagerError::PageLayoutInvalid {
        reason: message.into(),
    }
    .into()
}
