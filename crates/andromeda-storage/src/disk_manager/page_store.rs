use std::path::Path;

use andromeda_core::AndromedaResult;

use super::FileDiskManager;

/// Adapter to present FileDiskManager as a page-store façade.
pub struct DiskPageStore {
    manager: FileDiskManager,
}

impl DiskPageStore {
    pub fn new(file_path: impl AsRef<Path>, temp_dir: impl AsRef<Path>) -> AndromedaResult<Self> {
        let manager = FileDiskManager::open(file_path, temp_dir)?;
        Ok(Self { manager })
    }

    pub fn disk_manager_mut(&mut self) -> &mut FileDiskManager {
        &mut self.manager
    }
}
