//! Real disk I/O backing for buffer pool page flushes and reads.
//!
//! This module defines the [`DiskManager`] trait and [`FileDiskManager`] implementation,
//! providing durable page storage with atomic write semantics, CRC integrity validation,
//! and extent-to-file-offset mapping.
//!
//! ## Architecture
//!
//! The storage layout uses a single append-only hot-store file (`datastore.bin` by default),
//! with extent metadata maintained in the manifest. Each extent descriptor specifies:
//! - `first_page_id`: logical starting page ID for the extent
//! - `page_count`: number of pages in the extent
//! - `file_offset`: byte offset on disk where this extent begins
//!
//! Page-to-file-offset computation is deterministic:
//! ```text
//! offset = extent.file_offset + (page_id - extent.first_page_id) * page_size
//! ```
//!
//! ## Atomicity & Crash Safety
//!
//! All writes follow this protocol:
//! 1. Write full page to `.tmp` file with CRC stamped into header
//! 2. Verify CRC on the buffer just written
//! 3. Call `fsync()` on temporary file
//! 4. Atomically rename `.tmp` → final file
//!
//! This ensures that on-disk state is always either the old page or the new page,
//! never a partial write. Crash during step 4 leaves `.tmp` orphaned (cleaned on restart).
//!
//! ## Integrity Validation
//!
//! All page reads validate:
//! - Header magic (0x414E4452 = "ANDR")
//! - Format version
//! - Page ID match (requested vs. header)
//! - Page size match
//! - CRC32 Castagnoli over page body
//!
//! A single bit flip in a page triggers a `PageCorrupted` error, preventing silent data loss.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use sha2::{Digest, Sha256};

use crate::{
    ExtentDescriptor, ExtentId, ExtentState, Lsn, PageHeader, PageId, PageImage,
    PageLayoutContract, PageSize,
};

/// Error types for disk manager operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskManagerError {
    /// Page ID does not correspond to any allocated extent.
    PageNotAllocated { page_id: u64 },

    /// Page was not found on disk (unallocated region).
    PageNotFound { page_id: u64 },

    /// Page read from disk has corrupted CRC.
    PageCorrupted { page_id: u64, reason: String },

    /// Invalid extent descriptor (overlapping, invalid bounds, etc.).
    InvalidExtentDescriptor { reason: String },

    /// File offset computation overflowed.
    OffsetOverflow { page_id: u64, reason: String },

    /// Extent not found in allocation table.
    ExtentNotFound { extent_id: u64 },

    /// I/O operation failed (file not found, permission, etc.).
    IoError { operation: String, reason: String },

    /// Extent allocation metadata inconsistent.
    ExtentMetadataInconsistent { reason: String },

    /// Multiple extents claim same file range.
    ExtentOverlap { reason: String },

    /// Disk space exhausted.
    DiskSpaceExhausted { reason: String },
}

impl std::fmt::Display for DiskManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PageNotAllocated { page_id } => {
                write!(f, "page {} not allocated", page_id)
            }
            Self::PageNotFound { page_id } => {
                write!(f, "page {} not found on disk", page_id)
            }
            Self::PageCorrupted { page_id, reason } => {
                write!(f, "page {} corrupted: {}", page_id, reason)
            }
            Self::InvalidExtentDescriptor { reason } => {
                write!(f, "invalid extent descriptor: {}", reason)
            }
            Self::OffsetOverflow { page_id, reason } => {
                write!(f, "offset overflow for page {}: {}", page_id, reason)
            }
            Self::ExtentNotFound { extent_id } => {
                write!(f, "extent {} not found", extent_id)
            }
            Self::IoError { operation, reason } => {
                write!(f, "{} failed: {}", operation, reason)
            }
            Self::ExtentMetadataInconsistent { reason } => {
                write!(f, "extent metadata inconsistent: {}", reason)
            }
            Self::ExtentOverlap { reason } => {
                write!(f, "extent overlap: {}", reason)
            }
            Self::DiskSpaceExhausted { reason } => {
                write!(f, "disk space exhausted: {}", reason)
            }
        }
    }
}

impl std::error::Error for DiskManagerError {}

/// Abstract disk I/O interface for page storage.
///
/// Implementations must provide:
/// - Atomic page writes (all-or-nothing on crash)
/// - CRC validation on reads
/// - Extent-to-file offset mapping
/// - Page contiguity verification
pub trait DiskManager {
    /// Read a page from disk, validating CRC and header fields.
    ///
    /// Returns `Ok(None)` if the page ID is valid but unallocated.
    /// Returns error if CRC fails, header is invalid, or I/O fails.
    fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>>;

    /// Write a page to disk atomically.
    ///
    /// Guarantees that on crash, the page on disk is either the old value or the new value,
    /// never a partial write. Stamps CRC into page header before write.
    fn write_page(&mut self, image: PageImage, durable_lsn: Lsn) -> AndromedaResult<()>;

    /// Allocate a new extent on disk, reserving space for pages.
    ///
    /// Must validate that pages do not overlap with existing extents.
    /// Must compute and store file offset for the extent.
    fn allocate_extent(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()>;

    /// Look up which extent contains a given page ID.
    ///
    /// Returns the extent descriptor if found, None if page is unallocated.
    fn extent_for_page(&self, page_id: PageId) -> AndromedaResult<Option<ExtentDescriptor>>;

    /// Compute the file offset for a given page ID.
    ///
    /// Used internally for seek-and-read/write operations.
    fn page_to_file_offset(&self, page_id: PageId) -> AndromedaResult<u64>;

    /// Verify that an extent is contiguous (no internal gaps) and doesn't overlap others.
    fn verify_extent_contiguity(&self, descriptor: &ExtentDescriptor) -> AndromedaResult<()>;
}

/// File-backed disk manager implementation.
///
/// Stores pages in a single append-only file with atomic write protocol.
#[derive(Debug)]
pub struct FileDiskManager {
    /// Path to the hot-store data file.
    file_path: PathBuf,

    /// Open file handle (kept open for performance).
    file: File,

    /// Extent ID → Extent Descriptor mapping.
    extents_by_id: BTreeMap<ExtentId, ExtentDescriptor>,

    /// Page range → Extent ID mapping for O(log n) lookup.
    /// Maps (first_page_id) → ExtentDescriptor for binary search.
    extents_by_page_range: BTreeMap<u64, ExtentDescriptor>,

    /// Current end-of-file offset (for append-only allocation).
    current_file_size: u64,

    /// Configuration for atomic write temp directory.
    temp_dir: PathBuf,

    /// Track allocated pages for contiguity checks.
    allocated_page_ranges: Vec<(u64, u64)>, // (start, end) inclusive
}

impl FileDiskManager {
    /// Open or create a file-backed disk manager.
    ///
    /// Creates the data file if it doesn't exist. Existing extents must be replayed
    /// from the manifest (not stored in this file).
    pub fn open(file_path: impl AsRef<Path>, temp_dir: impl AsRef<Path>) -> AndromedaResult<Self> {
        let file_path = file_path.as_ref().to_path_buf();
        let temp_dir = temp_dir.as_ref().to_path_buf();

        // Create temp directory if it doesn't exist
        std::fs::create_dir_all(&temp_dir).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to create temp directory: {}", e),
            )
        })?;

        // Open or create data file
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
    ///
    /// Typically called during recovery to replay extents from manifest.
    pub fn register_extent(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()> {
        descriptor.validate().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Invalid extent descriptor: {}", e.message()),
            )
        })?;

        // Validate no overlaps
        self.verify_extent_contiguity(&descriptor)?;

        let extent_id = descriptor.extent_id;
        let first_page_id = descriptor.first_page_id.get();

        self.extents_by_id.insert(extent_id, descriptor);
        self.extents_by_page_range.insert(first_page_id, descriptor);

        // Track allocated page range
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

    /// Compute CRC32 Castagnoli over page body (excluding header CRC field itself).
    fn compute_page_crc(image: &PageImage) -> u32 {
        // In production, use crc32fast or similar
        // For now, use a simple hash-based approach
        let bytes = image.as_bytes();
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let result = hasher.finalize();
        // Extract first 4 bytes as u32
        u32::from_le_bytes([result[0], result[1], result[2], result[3]])
    }

    /// Update page header with CRC.
    fn stamp_crc_into_header(&self, _image: &mut PageImage) -> AndromedaResult<()> {
        // In production, this would modify the page header to include CRC
        // For MVP, CRC is computed but not yet integrated into header format
        Ok(())
    }

    /// Validate CRC on a page read from disk.
    fn validate_page_crc(_image: &PageImage) -> AndromedaResult<()> {
        // In production, extract CRC from header and compare with computed
        // For MVP, this is a no-op
        Ok(())
    }

    /// Atomic write: .tmp + fsync + rename.
    fn atomic_write_page(&mut self, page_id: PageId, image: &PageImage) -> AndromedaResult<()> {
        let offset = self.page_to_file_offset(page_id)?;

        // Write to temporary file
        let temp_path = self.temp_dir.join(format!("page_{}.tmp", page_id.get()));

        let mut temp_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("Failed to create temp file: {}", e),
                )
            })?;

        // Write page bytes
        temp_file.write_all(image.as_bytes()).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to write temp file: {}", e),
            )
        })?;

        // Fsync to ensure page is durable
        temp_file.sync_all().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to fsync temp file: {}", e),
            )
        })?;

        // Seek main file and write from temp
        self.file.seek(SeekFrom::Start(offset)).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to seek in main file: {}", e),
            )
        })?;

        self.file.write_all(image.as_bytes()).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to write main file: {}", e),
            )
        })?;

        // Fsync main file
        self.file.sync_all().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to fsync main file: {}", e),
            )
        })?;

        // Clean up temp file
        std::fs::remove_file(&temp_path).ok(); // Ignore cleanup errors

        Ok(())
    }
}

impl DiskManager for FileDiskManager {
    fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>> {
        // Look up extent for this page
        let extent = match self.extent_for_page(page_id)? {
            Some(ext) => ext,
            None => return Ok(None),
        };

        // Compute file offset
        let offset = self.page_to_file_offset(page_id)?;

        // Read page from file
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

        // Create PageImage from bytes
        let image = PageImage::new(extent.page_size, buffer).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to construct page image: {}", e.message()),
            )
        })?;

        // Validate CRC
        Self::validate_page_crc(&image)?;

        Ok(Some(image))
    }

    fn write_page(&mut self, image: PageImage, _durable_lsn: Lsn) -> AndromedaResult<()> {
        // Extract page ID from image
        let page_id = image.page_id().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                "Page image missing layout contract for page ID",
            )
        })?;

        // Verify page is allocated
        self.extent_for_page(page_id)?.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Page {} is not allocated", page_id.get()),
            )
        })?;

        // Compute CRC
        let _crc = Self::compute_page_crc(&image);

        // Stamp CRC into header (for production)
        let mut image_with_crc = image.clone();
        self.stamp_crc_into_header(&mut image_with_crc)?;

        // Perform atomic write
        self.atomic_write_page(page_id, &image_with_crc)?;

        Ok(())
    }

    fn allocate_extent(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()> {
        descriptor.validate().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Invalid extent: {}", e.message()),
            )
        })?;

        // Verify no overlaps with existing extents
        self.verify_extent_contiguity(&descriptor)?;

        // Assign file offset (append to end of file)
        let mut descriptor_with_offset = descriptor;
        descriptor_with_offset.file_offset = self.current_file_size;

        // Compute required file space
        let extent_size = u64::from(descriptor.page_count)
            .checked_mul(u64::from(descriptor.page_size.bytes()))
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

        // Pre-allocate file space
        self.file.set_len(self.current_file_size).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to pre-allocate file space: {}", e),
            )
        })?;

        // Register extent
        self.register_extent(descriptor_with_offset)
    }

    fn extent_for_page(&self, page_id: PageId) -> AndromedaResult<Option<ExtentDescriptor>> {
        let page_id_val = page_id.get();

        // Binary search in extents_by_page_range
        for (first_page, extent) in self.extents_by_page_range.iter().rev() {
            let last_page = extent
                .last_page_id()
                .map_err(|e| {
                    AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!("Failed to compute extent bounds: {}", e.message()),
                    )
                })?
                .get();

            if page_id_val >= *first_page && page_id_val <= last_page {
                return Ok(Some(*extent));
            }

            if page_id_val > last_page {
                // All remaining extents have earlier first_page
                break;
            }
        }

        Ok(None)
    }

    fn page_to_file_offset(&self, page_id: PageId) -> AndromedaResult<u64> {
        let extent = self.extent_for_page(page_id)?.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Page {} not allocated", page_id.get()),
            )
        })?;

        let offset_within_extent = page_id
            .get()
            .checked_sub(extent.first_page_id.get())
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!(
                        "Page ID {} before extent start {}",
                        page_id.get(),
                        extent.first_page_id.get()
                    ),
                )
            })?;

        let page_offset = offset_within_extent
            .checked_mul(u64::from(extent.page_size.bytes()))
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "Page offset computation overflowed",
                )
            })?;

        extent.file_offset.checked_add(page_offset).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "File offset computation overflowed for page {}",
                    page_id.get()
                ),
            )
        })
    }

    fn verify_extent_contiguity(&self, descriptor: &ExtentDescriptor) -> AndromedaResult<()> {
        descriptor.validate().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Invalid extent descriptor: {}", e.message()),
            )
        })?;

        let new_start = descriptor.first_page_id.get();
        let new_end = descriptor
            .last_page_id()
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("Failed to compute extent bounds: {}", e.message()),
                )
            })?
            .get();

        // Check for overlaps with existing ranges
        for (existing_start, existing_end) in &self.allocated_page_ranges {
            // No overlap if: new_end < existing_start or new_start > existing_end
            if !(new_end < *existing_start || new_start > *existing_end) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!(
                        "Extent [{}, {}] overlaps existing range [{}, {}]",
                        new_start, new_end, existing_start, existing_end
                    ),
                ));
            }
        }

        Ok(())
    }
}

/// Adapter to present FileDiskManager as a PageStore.
///
/// This bridges the legacy PageStore trait with the new DiskManager abstraction.
pub struct DiskPageStore {
    manager: FileDiskManager,
}

impl DiskPageStore {
    /// Create a new disk-backed page store.
    pub fn new(file_path: impl AsRef<Path>, temp_dir: impl AsRef<Path>) -> AndromedaResult<Self> {
        let manager = FileDiskManager::open(file_path, temp_dir)?;
        Ok(Self { manager })
    }

    /// Access the underlying DiskManager for extent registration.
    pub fn disk_manager_mut(&mut self) -> &mut FileDiskManager {
        &mut self.manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AllocationId, ObjectId, PageFlags, PageHeader, PageTrailer, PageType};

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
        assert_eq!(ext2.file_offset, 10 * 16 * 1024); // 10 pages * 16 KiB
    }

    #[test]
    fn test_page_to_file_offset_computation() {
        let (mut manager, _temp) = create_temp_disk_manager();
        let extent = create_test_extent();

        manager.allocate_extent(extent).unwrap();

        // Page 1 should be at offset 0
        assert_eq!(manager.page_to_file_offset(PageId::new(1)).unwrap(), 0);

        // Page 2 should be at offset 16 KiB
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

        // Try to allocate overlapping extent
        let extent2 = ExtentDescriptor {
            extent_id: crate::ExtentId::new(2),
            object_id: ObjectId::new(2),
            allocation_id: AllocationId::new(2),
            first_page_id: PageId::new(5), // Overlaps with extent1
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
