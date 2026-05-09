use andromeda_error::AndromedaResult;
use andromeda_segment::ExtentDescriptor;
use andromeda_storage_page::{Lsn, PageId, PageImage};

/// Abstract disk I/O interface for page storage.
pub trait DiskManager {
    /// Read a page from disk.
    ///
    /// Returns `Ok(None)` if the page ID is valid but unallocated.
    fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>>;

    /// Write a page to disk durably.
    fn write_page(&mut self, image: PageImage, durable_lsn: Lsn) -> AndromedaResult<()>;

    /// Allocate a new extent on disk.
    fn allocate_extent(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()>;

    /// Look up which extent contains a given page ID.
    fn extent_for_page(&self, page_id: PageId) -> AndromedaResult<Option<ExtentDescriptor>>;

    /// Compute the file offset for a given page ID.
    fn page_to_file_offset(&self, page_id: PageId) -> AndromedaResult<u64>;

    /// Verify that an extent is contiguous and doesn't overlap others.
    fn verify_extent_contiguity(&self, descriptor: &ExtentDescriptor) -> AndromedaResult<()>;
}
