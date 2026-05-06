use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{ExtentDescriptor, ExtentId, Lsn, PageId, PageImage};

use super::interface::DiskManager;

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
}

impl FileDiskManager {
    /// Open or create a file-backed disk manager.
    pub fn open(file_path: impl AsRef<Path>, temp_dir: impl AsRef<Path>) -> AndromedaResult<Self> {
        let file_path = file_path.as_ref().to_path_buf();
        let temp_dir = temp_dir.as_ref().to_path_buf();

        std::fs::create_dir_all(&temp_dir).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to create temp directory: {}", e),
            )
        })?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&file_path)
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!(
                        "Failed to open disk manager file {}: {}",
                        file_path.display(),
                        e
                    ),
                )
            })?;

        let current_file_size = file
            .metadata()
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("Failed to read file metadata: {}", e),
                )
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
        })
    }

    /// Register an extent with this disk manager.
    pub fn register_extent(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()> {
        descriptor.validate().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Invalid extent descriptor: {}", e.message()),
            )
        })?;

        self.verify_extent_contiguity_impl(&descriptor)?;

        let extent_id = descriptor.extent_id;
        let first_page_id = descriptor.first_page_id.get();

        self.extents_by_id.insert(extent_id, descriptor);
        self.extents_by_page_range.insert(first_page_id, descriptor);

        let last_page_id = descriptor
            .last_page_id()
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("Failed to compute extent last page: {}", e.message()),
                )
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

        let mut buffer = vec![0u8; extent.page_size.bytes_usize()];
        let mut file = OpenOptions::new()
            .read(true)
            .open(&self.file_path)
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("Failed to open data file for reading: {}", e),
                )
            })?;

        file.seek(SeekFrom::Start(offset)).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "Failed to seek to page {} (offset {}): {}",
                    page_id.get(),
                    offset,
                    e
                ),
            )
        })?;

        file.read_exact(&mut buffer).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "Failed to read page {} (offset {}): {}",
                    page_id.get(),
                    offset,
                    e
                ),
            )
        })?;

        let image = PageImage::new(extent.page_size, buffer).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to construct page image: {}", e.message()),
            )
        })?;

        self.validate_page_integrity(&image)?;

        Ok(Some(image))
    }

    fn write_page(&mut self, image: PageImage, _durable_lsn: Lsn) -> AndromedaResult<()> {
        let page_id = image.page_id().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                "Page image missing layout contract for page ID",
            )
        })?;

        self.extent_for_page_impl(page_id)?.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Page {} is not allocated", page_id.get()),
            )
        })?;

        let mut image_for_write = image.clone();
        self.stamp_page_integrity_if_enabled(&mut image_for_write)?;
        self.atomic_write_page(page_id, &image_for_write)?;
        Ok(())
    }

    fn allocate_extent(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()> {
        descriptor.validate().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Invalid extent: {}", e.message()),
            )
        })?;

        self.verify_extent_contiguity_impl(&descriptor)?;

        let mut descriptor_with_offset = descriptor;
        descriptor_with_offset.file_offset = self.current_file_size;

        let extent_size = u64::from(descriptor_with_offset.page_count)
            .checked_mul(u64::from(descriptor_with_offset.page_size.bytes()))
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "Extent size computation overflowed",
                )
            })?;

        self.current_file_size = descriptor_with_offset
            .file_offset
            .checked_add(extent_size)
            .ok_or_else(|| {
                AndromedaError::new(AndromedaErrorKind::Storage, "File size would overflow")
            })?;

        self.file.set_len(self.current_file_size).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to pre-allocate file space: {}", e),
            )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk_manager::DiskManager;
    use crate::{AllocationId, ExtentState, ObjectId, PageSize};

    fn create_temp_disk_manager() -> (FileDiskManager, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let data_file = temp_dir.path().join("test.bin");
        let manager = FileDiskManager::open(&data_file, temp_dir.path()).unwrap();
        (manager, temp_dir)
    }

    fn create_test_extent() -> ExtentDescriptor {
        ExtentDescriptor {
            extent_id: crate::ExtentId::new(1),
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

    #[test]
    fn test_disk_manager_open_creates_file() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let data_file = temp_dir.path().join("test.bin");

        assert!(!data_file.exists());
        let _manager = FileDiskManager::open(&data_file, temp_dir.path()).unwrap();
        assert!(data_file.exists());
    }

    #[test]
    fn test_extent_allocation_assigns_offset() {
        let (mut manager, _temp) = create_temp_disk_manager();
        let extent = create_test_extent();

        manager.allocate_extent(extent).unwrap();

        let registered = manager.extent_for_page(PageId::new(1)).unwrap().unwrap();
        assert_eq!(registered.file_offset, 0);
    }

    #[test]
    fn test_sequential_extents_append_contiguously() {
        let (mut manager, _temp) = create_temp_disk_manager();

        let extent1 = ExtentDescriptor {
            extent_id: crate::ExtentId::new(1),
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

        manager.allocate_extent(extent1).unwrap();

        let extent2 = ExtentDescriptor {
            extent_id: crate::ExtentId::new(2),
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

        manager.allocate_extent(extent2).unwrap();

        let ext2 = manager.extent_for_page(PageId::new(11)).unwrap().unwrap();
        assert_eq!(ext2.file_offset, 10 * 16 * 1024);
    }

    #[test]
    fn test_page_to_file_offset_computation() {
        let (mut manager, _temp) = create_temp_disk_manager();
        let extent = create_test_extent();

        manager.allocate_extent(extent).unwrap();

        assert_eq!(manager.page_to_file_offset(PageId::new(1)).unwrap(), 0);
        assert_eq!(
            manager.page_to_file_offset(PageId::new(2)).unwrap(),
            16 * 1024
        );
    }

    #[test]
    fn test_overlapping_extents_rejected() {
        let (mut manager, _temp) = create_temp_disk_manager();

        let extent1 = ExtentDescriptor {
            extent_id: crate::ExtentId::new(1),
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

        manager.allocate_extent(extent1).unwrap();

        let extent2 = ExtentDescriptor {
            extent_id: crate::ExtentId::new(2),
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
    }
}
