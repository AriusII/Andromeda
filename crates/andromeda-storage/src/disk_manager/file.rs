use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use andromeda_core::AndromedaResult;
use andromeda_disk_page_store::PageFlushDurabilityBoundary;

use crate::{ExtentDescriptor, ExtentId, Lsn, PageId, PageImage};

use super::interface::DiskManager;
use super::{DiskManagerError, PageIntegrityMode};

const MAX_FILE_PAGE_READ_BYTES: usize = 32 * 1024;

/// File-backed disk manager implementation.
#[derive(Debug)]
pub struct FileDiskManager {
    pub(crate) file_path: PathBuf,
    pub(crate) file: File,
    pub(crate) extents_by_id: BTreeMap<ExtentId, ExtentDescriptor>,
    pub(crate) extents_by_page_range: BTreeMap<u64, ExtentDescriptor>,
    pub(crate) current_file_size: u64,
    pub(crate) temp_dir: PathBuf,
    pub(crate) allocated_page_ranges: Vec<(u64, u64)>, // (start, end) inclusive
    pub(crate) page_integrity_mode: PageIntegrityMode,
}

impl FileDiskManager {
    /// Open or create a file-backed disk manager.
    pub fn open(file_path: impl AsRef<Path>, temp_dir: impl AsRef<Path>) -> AndromedaResult<Self> {
        let file_path = file_path.as_ref().to_path_buf();
        let temp_dir = temp_dir.as_ref().to_path_buf();

        std::fs::create_dir_all(&temp_dir).map_err(|e| DiskManagerError::IoError {
            operation: format!("create disk manager temp directory {}", temp_dir.display()),
            reason: e.to_string(),
        })?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&file_path)
            .map_err(|e| DiskManagerError::IoError {
                operation: format!("open disk manager file {}", file_path.display()),
                reason: e.to_string(),
            })?;

        let current_file_size = file
            .metadata()
            .map_err(|e| DiskManagerError::IoError {
                operation: format!("read disk manager metadata {}", file_path.display()),
                reason: e.to_string(),
            })?
            .len();

        Ok(Self {
            file_path,
            file,
            extents_by_id: BTreeMap::new(),
            extents_by_page_range: BTreeMap::new(),
            current_file_size,
            temp_dir,
            allocated_page_ranges: Vec::new(),
            page_integrity_mode: PageIntegrityMode::None,
        })
    }

    /// Open or create a file-backed disk manager with explicit page integrity.
    pub fn open_with_integrity(
        file_path: impl AsRef<Path>,
        temp_dir: impl AsRef<Path>,
        page_integrity_mode: PageIntegrityMode,
    ) -> AndromedaResult<Self> {
        let mut manager = Self::open(file_path, temp_dir)?;
        manager.page_integrity_mode = page_integrity_mode;
        Ok(manager)
    }

    /// Register an extent with this disk manager.
    pub fn register_extent(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()> {
        descriptor
            .validate()
            .map_err(|e| DiskManagerError::InvalidExtentDescriptor {
                reason: e.message().to_string(),
            })?;

        self.verify_extent_contiguity_impl(&descriptor)?;

        let extent_id = descriptor.extent_id;
        let first_page_id = descriptor.first_page_id.get();

        self.extents_by_id.insert(extent_id, descriptor);
        self.extents_by_page_range.insert(first_page_id, descriptor);

        let last_page_id = descriptor
            .last_page_id()
            .map_err(|e| DiskManagerError::InvalidExtentDescriptor {
                reason: format!("failed to compute last page: {}", e.message()),
            })?
            .get();

        self.allocated_page_ranges
            .push((first_page_id, last_page_id));
        self.allocated_page_ranges.sort_unstable();
        Ok(())
    }
}

impl DiskManager for FileDiskManager {
    fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>> {
        let extent = match self.extent_for_page_impl(page_id)? {
            Some(extent) => extent,
            None => return Ok(None),
        };

        let offset = self.page_to_file_offset_impl(page_id)?;

        let page_read_len = bounded_page_read_len(extent.page_size.bytes_usize(), page_id)?;
        let mut buffer = vec![0u8; page_read_len];
        let mut file = OpenOptions::new()
            .read(true)
            .open(&self.file_path)
            .map_err(|e| DiskManagerError::IoError {
                operation: format!("open data file {} for reading", self.file_path.display()),
                reason: e.to_string(),
            })?;

        file.seek(SeekFrom::Start(offset))
            .map_err(|e| DiskManagerError::IoError {
                operation: format!("seek to page {} at offset {}", page_id.get(), offset),
                reason: e.to_string(),
            })?;

        file.read_exact(&mut buffer)
            .map_err(|e| DiskManagerError::IoError {
                operation: format!("read page {} at offset {}", page_id.get(), offset),
                reason: e.to_string(),
            })?;

        let image = PageImage::new(extent.page_size, buffer).map_err(|e| {
            DiskManagerError::PageCorrupted {
                page_id: page_id.get(),
                reason: format!("failed to construct page image: {}", e.message()),
            }
        })?;

        self.validate_page_integrity(&image)?;

        Ok(Some(image))
    }

    fn write_page(&mut self, image: PageImage, durable_lsn: Lsn) -> AndromedaResult<()> {
        let page_id = image
            .page_id()
            .ok_or_else(|| DiskManagerError::PageLayoutInvalid {
                reason: "page image missing layout contract for page ID".to_string(),
            })?;
        let page_lsn = image
            .page_lsn()
            .ok_or_else(|| DiskManagerError::PageLayoutInvalid {
                reason: "page image missing layout contract for page LSN".to_string(),
            })?;
        validate_page_flush_boundary(page_lsn, durable_lsn)?;

        self.extent_for_page_impl(page_id)?
            .ok_or_else(|| DiskManagerError::PageNotAllocated {
                page_id: page_id.get(),
            })?;

        let mut image_for_write = image;
        self.stamp_page_integrity_if_enabled(&mut image_for_write)?;
        self.atomic_write_page(page_id, &image_for_write)?;
        Ok(())
    }

    fn allocate_extent(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()> {
        descriptor
            .validate()
            .map_err(|e| DiskManagerError::InvalidExtentDescriptor {
                reason: e.message().to_string(),
            })?;

        self.verify_extent_contiguity_impl(&descriptor)?;

        let mut descriptor_with_offset = descriptor;
        descriptor_with_offset.file_offset = self.current_file_size;

        let extent_size = u64::from(descriptor_with_offset.page_count)
            .checked_mul(u64::from(descriptor_with_offset.page_size.bytes()))
            .ok_or_else(|| DiskManagerError::OffsetOverflow {
                page_id: descriptor_with_offset.first_page_id.get(),
                reason: "extent size computation overflowed".to_string(),
            })?;

        self.current_file_size = descriptor_with_offset
            .file_offset
            .checked_add(extent_size)
            .ok_or_else(|| DiskManagerError::DiskSpaceExhausted {
                reason: "file size would overflow".to_string(),
            })?;

        self.file
            .set_len(self.current_file_size)
            .map_err(|e| DiskManagerError::IoError {
                operation: format!(
                    "pre-allocate file space to {} bytes",
                    self.current_file_size
                ),
                reason: e.to_string(),
            })?;

        self.register_extent(descriptor_with_offset)
    }

    fn extent_for_page(&self, page_id: PageId) -> AndromedaResult<Option<ExtentDescriptor>> {
        self.extent_for_page_impl(page_id)
    }

    fn page_to_file_offset(&self, page_id: PageId) -> AndromedaResult<u64> {
        self.page_to_file_offset_impl(page_id)
    }

    fn verify_extent_contiguity(&self, descriptor: &ExtentDescriptor) -> AndromedaResult<()> {
        self.verify_extent_contiguity_impl(descriptor)
    }
}

fn validate_page_flush_boundary(page_lsn: Lsn, durable_lsn: Lsn) -> AndromedaResult<()> {
    PageFlushDurabilityBoundary::new(page_lsn, durable_lsn)
        .validate()
        .map_err(|error| match error {
            andromeda_disk_page_store::PageFlushDurabilityError::MissingPageLsn => {
                DiskManagerError::PageLayoutInvalid {
                    reason: "page flush requires a nonzero page LSN".to_string(),
                }
                .into()
            }
            andromeda_disk_page_store::PageFlushDurabilityError::WalFenceViolation {
                page_lsn,
                durable_lsn,
            } => DiskManagerError::WalFenceViolation {
                page_lsn,
                durable_lsn,
            }
            .into(),
        })
}

fn bounded_page_read_len(page_bytes: usize, page_id: PageId) -> AndromedaResult<usize> {
    if page_bytes == 0 || page_bytes > MAX_FILE_PAGE_READ_BYTES {
        return Err(DiskManagerError::PageLayoutInvalid {
            reason: format!(
                "page {} read length {} exceeds fixed disk manager page budget {}",
                page_id.get(),
                page_bytes,
                MAX_FILE_PAGE_READ_BYTES
            ),
        }
        .into());
    }
    Ok(page_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk_manager::{DiskManager, DiskManagerError};
    use crate::{
        AllocationId, ExtentState, ObjectId, PageFlags, PageHeader, PageLayoutContract, PageSize,
        PageTrailer, PageType,
    };

    fn create_temp_disk_manager() -> AndromedaResult<(FileDiskManager, tempfile::TempDir)> {
        let temp_dir = tempfile::TempDir::new().map_err(|e| DiskManagerError::IoError {
            operation: "create test temp directory".to_string(),
            reason: e.to_string(),
        })?;
        let data_file = temp_dir.path().join("test.bin");
        let manager = FileDiskManager::open(&data_file, temp_dir.path())?;
        Ok((manager, temp_dir))
    }

    fn create_test_extent() -> ExtentDescriptor {
        ExtentDescriptor {
            extent_id: ExtentId::new(1),
            object_id: ObjectId::new(1),
            allocation_id: AllocationId::new(1),
            first_page_id: PageId::new(1),
            page_count: 10,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        }
    }

    fn valid_page(page_id: PageId, page_lsn: Lsn) -> PageImage {
        let layout = PageLayoutContract {
            header: PageHeader {
                magic: PageHeader::MAGIC,
                format_version: PageHeader::FORMAT_VERSION_V0,
                page_size: crate::PageSize::KiB16,
                page_type: PageType::FixedRow,
                page_id,
                object_id: ObjectId::new(1),
                allocation_id: AllocationId::new(1),
                page_lsn,
                page_epoch: 1,
                previous_page_id: None,
                next_page_id: None,
                header_len: PageHeader::MIN_HEADER_LEN_V0,
                payload_offset: 128,
                payload_len: 512,
                free_start: 256,
                free_end: 512,
                free_bytes: 256,
                slot_count: 1,
                row_count: 1,
                flags: PageFlags::NONE,
                header_crc: 5,
            },
            trailer: PageTrailer {
                payload_crc64: 6,
                page_hash: [7; 32],
                torn_write_guard: 8,
            },
        };
        PageImage::with_layout(layout, vec![0; crate::PageSize::KiB16.bytes_usize()])
            .expect("valid page image")
    }

    #[test]
    fn test_disk_manager_open_creates_file() -> AndromedaResult<()> {
        let temp_dir = tempfile::TempDir::new().map_err(|e| DiskManagerError::IoError {
            operation: "create test temp directory".to_string(),
            reason: e.to_string(),
        })?;
        let data_file = temp_dir.path().join("test.bin");

        assert!(!data_file.exists());
        let _manager = FileDiskManager::open(&data_file, temp_dir.path())?;
        assert!(data_file.exists());
        Ok(())
    }

    #[test]
    fn test_extent_allocation_assigns_offset() -> AndromedaResult<()> {
        let (mut manager, _temp) = create_temp_disk_manager()?;
        let extent = create_test_extent();

        manager.allocate_extent(extent)?;

        let registered = manager
            .extent_for_page(PageId::new(1))?
            .ok_or(DiskManagerError::PageNotFound { page_id: 1 })?;
        assert_eq!(registered.file_offset, 0);
        Ok(())
    }

    #[test]
    fn test_sequential_extents_append_contiguously() -> AndromedaResult<()> {
        let (mut manager, _temp) = create_temp_disk_manager()?;

        let extent1 = ExtentDescriptor {
            extent_id: ExtentId::new(1),
            object_id: ObjectId::new(1),
            allocation_id: AllocationId::new(1),
            first_page_id: PageId::new(1),
            page_count: 10,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        };

        manager.allocate_extent(extent1)?;

        let extent2 = ExtentDescriptor {
            extent_id: ExtentId::new(2),
            object_id: ObjectId::new(2),
            allocation_id: AllocationId::new(2),
            first_page_id: PageId::new(11),
            page_count: 5,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        };

        manager.allocate_extent(extent2)?;

        let ext2 = manager
            .extent_for_page(PageId::new(11))?
            .ok_or(DiskManagerError::PageNotFound { page_id: 11 })?;
        assert_eq!(ext2.file_offset, 10 * 16 * 1024);
        Ok(())
    }

    #[test]
    fn test_page_to_file_offset_computation() -> AndromedaResult<()> {
        let (mut manager, _temp) = create_temp_disk_manager()?;
        let extent = create_test_extent();

        manager.allocate_extent(extent)?;

        assert_eq!(manager.page_to_file_offset(PageId::new(1))?, 0);
        assert_eq!(manager.page_to_file_offset(PageId::new(2))?, 16 * 1024);
        Ok(())
    }

    #[test]
    fn test_write_page_rejects_page_lsn_ahead_of_durable_wal() -> AndromedaResult<()> {
        let (mut manager, _temp) = create_temp_disk_manager()?;
        manager.allocate_extent(create_test_extent())?;

        let page = valid_page(PageId::new(1), Lsn::new(100));
        let error = manager
            .write_page(page.clone(), Lsn::new(99))
            .expect_err("page write must wait for durable WAL through page LSN");

        assert!(error.message().contains("WAL-before-page flush violated"));
        manager
            .write_page(page, Lsn::new(100))
            .expect("page write succeeds once WAL is durable through page LSN");
        Ok(())
    }

    #[test]
    fn test_bounded_page_read_len_rejects_oversized_extent_page() {
        let error = bounded_page_read_len(MAX_FILE_PAGE_READ_BYTES + 1, PageId::new(1))
            .expect_err("disk manager page reads must stay within the fixed page budget");

        assert!(error.message().contains("fixed disk manager page budget"));
    }

    #[test]
    fn test_overlapping_extents_rejected() -> AndromedaResult<()> {
        let (mut manager, _temp) = create_temp_disk_manager()?;

        let extent1 = ExtentDescriptor {
            extent_id: ExtentId::new(1),
            object_id: ObjectId::new(1),
            allocation_id: AllocationId::new(1),
            first_page_id: PageId::new(1),
            page_count: 10,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        };

        manager.allocate_extent(extent1)?;

        let extent2 = ExtentDescriptor {
            extent_id: ExtentId::new(2),
            object_id: ObjectId::new(2),
            allocation_id: AllocationId::new(2),
            first_page_id: PageId::new(5),
            page_count: 10,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        };

        let result = manager.allocate_extent(extent2);
        assert!(result.is_err());
        Ok(())
    }
}
