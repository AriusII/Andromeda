use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

use andromeda_core::AndromedaResult;

use crate::{PageId, PageImage};

use super::{DiskManagerError, FileDiskManager};

impl FileDiskManager {
    /// Durable write protocol:
    /// 1) write page bytes to temp file, 2) fsync temp file,
    /// 3) write bytes to fixed offset in main file, 4) fsync main file.
    pub(crate) fn atomic_write_page(
        &mut self,
        page_id: PageId,
        image: &PageImage,
    ) -> AndromedaResult<()> {
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
        Ok(())
    }
}
