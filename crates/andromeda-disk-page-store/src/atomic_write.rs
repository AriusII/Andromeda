use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

use andromeda_error::AndromedaResult;
use andromeda_storage_page::{PageId, PageImage};
use andromeda_wal::WalIoQueueClass;

use super::{DiskManagerError, FileDiskManager};

impl FileDiskManager {
    /// Durable write protocol:
    /// 1) write page bytes to temp file, 2) fsync temp file,
    /// 3) write bytes to fixed offset in main file, 4) fsync main file.
    ///
    /// # C5 invariant enforcement
    /// Temp/spill writes are classified [`WalIoQueueClass::P1Maintenance`] by default.
    /// This method returns [`DiskManagerError::QueueClassViolation`] if the
    /// manager's configured class is [`WalIoQueueClass::P0Durability`] — that class
    /// is reserved for the WAL→commit critical path and must never be used for
    /// temp/spill I/O.
    ///
    /// On success, [`FileDiskManager::temp_write_count`] is incremented by one.
    ///
    /// # Errors
    /// Returns [`DiskManagerError::QueueClassViolation`] if the manager's
    /// `temp_write_queue_class` is `P0Durability`.
    /// Returns [`DiskManagerError::IoError`] for any underlying I/O failure.
    pub(crate) fn atomic_write_page(
        &mut self,
        page_id: PageId,
        image: &PageImage,
    ) -> AndromedaResult<()> {
        // C5 invariant: temp/spill writes must never be classified as P0Durable.
        // P0Durability is reserved for the WAL→commit critical path only.
        // This check is defense-in-depth: `with_temp_write_class` already rejects P0
        // at construction time, and the default is P1Maintenance.
        if self.temp_write_queue_class == WalIoQueueClass::P0Durability {
            return Err(DiskManagerError::QueueClassViolation {
                reason: format!(
                    "atomic_write_page for page {} attempted with P0Durability queue class; \
                     temp/spill writes must use P1Maintenance — \
                     P0 is reserved for the WAL→commit critical path",
                    page_id.get()
                ),
            }
            .into());
        }

        let offset = self.page_to_file_offset_impl(page_id)?;
        let temp_path = self.temp_dir.join(format!("page_{}.tmp", page_id.get()));

        let mut temp_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)
            .map_err(|e| DiskManagerError::IoError {
                operation: format!("create temp page file {}", temp_path.display()),
                reason: e.to_string(),
            })?;

        temp_file
            .write_all(image.as_bytes())
            .map_err(|e| DiskManagerError::IoError {
                operation: format!("write temp page file {}", temp_path.display()),
                reason: e.to_string(),
            })?;

        temp_file
            .sync_all()
            .map_err(|e| DiskManagerError::IoError {
                operation: format!("fsync temp page file {}", temp_path.display()),
                reason: e.to_string(),
            })?;
        drop(temp_file);

        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| DiskManagerError::IoError {
                operation: format!(
                    "seek main file to page {} at offset {}",
                    page_id.get(),
                    offset
                ),
                reason: e.to_string(),
            })?;

        self.file
            .write_all(image.as_bytes())
            .map_err(|e| DiskManagerError::IoError {
                operation: format!("write page {} to main file", page_id.get()),
                reason: e.to_string(),
            })?;

        self.file
            .sync_all()
            .map_err(|e| DiskManagerError::IoError {
                operation: format!("fsync main file after writing page {}", page_id.get()),
                reason: e.to_string(),
            })?;

        let _ = std::fs::remove_file(&temp_path);
        // Increment the classified temp-write counter.  This is the observable
        // telemetry hook for integration tests that prove P1Maintenance writes
        // are counted and P0Durability writes are rejected (never reach here).
        self.temp_write_count = self.temp_write_count.saturating_add(1);
        Ok(())
    }
}
