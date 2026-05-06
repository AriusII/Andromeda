use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{PageId, PageImage};

use super::FileDiskManager;

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
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("Failed to create temp file {}: {}", temp_path.display(), e),
                )
            })?;

        temp_file.write_all(image.as_bytes()).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to write temp file: {}", e),
            )
        })?;

        temp_file.sync_all().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to fsync temp file: {}", e),
            )
        })?;
        drop(temp_file);

        self.file.seek(SeekFrom::Start(offset)).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to seek in main file to offset {}: {}", offset, e),
            )
        })?;

        self.file.write_all(image.as_bytes()).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Failed to write page {} to main file: {}", page_id.get(), e),
            )
        })?;

        self.file.sync_all().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "Failed to fsync main file after writing page {}: {}",
                    page_id.get(),
                    e
                ),
            )
        })?;

        let _ = std::fs::remove_file(&temp_path);
        Ok(())
    }
}
