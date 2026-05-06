use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use sha2::{Digest, Sha256};

use crate::PageImage;

use super::FileDiskManager;

/// Explicit integrity mode for durable pages.
///
/// The current default is disabled because page-header CRC is not yet persisted
/// inside `PageImage` bytes by the page codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageIntegrityMode {
    DisabledUntilPageHeaderCrcIntegrated,
    HeaderCrc32,
}

impl FileDiskManager {
    pub(crate) const fn page_integrity_mode(&self) -> PageIntegrityMode {
        PageIntegrityMode::DisabledUntilPageHeaderCrcIntegrated
    }

    /// Deterministic integrity tag retained for compatibility with existing page images.
    ///
    /// Note: this is not CRC32. It is the first 32 bits of SHA-256.
    fn compute_integrity_tag(image: &PageImage) -> u32 {
        let mut hasher = Sha256::new();
        hasher.update(image.as_bytes());
        let result = hasher.finalize();
        u32::from_le_bytes([result[0], result[1], result[2], result[3]])
    }

    pub(crate) fn stamp_page_integrity_if_enabled(
        &self,
        image: &mut PageImage,
    ) -> AndromedaResult<()> {
        match self.page_integrity_mode() {
            PageIntegrityMode::DisabledUntilPageHeaderCrcIntegrated => {
                let _ = image;
                Ok(())
            }
            PageIntegrityMode::HeaderCrc32 => {
                let Some(mut layout) = image.layout_contract() else {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "HeaderCrc32 integrity mode requires page layout contract",
                    ));
                };
                let tag = Self::compute_integrity_tag(image);
                layout.header.header_crc = if tag == 0 { 1 } else { tag };
                *image =
                    PageImage::with_layout(layout, image.as_bytes().to_vec()).map_err(|e| {
                        AndromedaError::new(
                            AndromedaErrorKind::Storage,
                            format!("Failed to stamp page integrity tag: {}", e.message()),
                        )
                    })?;
                Ok(())
            }
        }
    }

    pub(crate) fn validate_page_integrity(&self, image: &PageImage) -> AndromedaResult<()> {
        match self.page_integrity_mode() {
            PageIntegrityMode::DisabledUntilPageHeaderCrcIntegrated => Ok(()),
            PageIntegrityMode::HeaderCrc32 => {
                let Some(layout) = image.layout_contract() else {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "HeaderCrc32 integrity mode requires persisted page layout contract",
                    ));
                };
                let expected = layout.header.header_crc;
                let actual = Self::compute_integrity_tag(image);
                if expected != actual {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!(
                            "Page integrity check failed: expected header tag {expected:#010x}, got {actual:#010x}"
                        ),
                    ));
                }
                Ok(())
            }
        }
    }
}
