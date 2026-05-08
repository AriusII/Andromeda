use andromeda_core::AndromedaResult;

use crate::{PageHeader, PageImage};

use super::{DiskManagerError, FileDiskManager};

pub use andromeda_disk_page_store::PageIntegrityMode;

impl FileDiskManager {
    pub(crate) const HEADER_CRC_OFFSET: usize = 94;
    const HEADER_CRC_LEN: usize = 4;

    pub const fn page_integrity_mode(&self) -> PageIntegrityMode {
        self.page_integrity_mode
    }

    fn compute_integrity_crc32(bytes: &[u8]) -> AndromedaResult<u32> {
        let mut crc = 0xFFFF_FFFFu32;
        for (offset, byte) in bytes.iter().copied().enumerate() {
            let byte = if (Self::HEADER_CRC_OFFSET..Self::HEADER_CRC_OFFSET + Self::HEADER_CRC_LEN)
                .contains(&offset)
            {
                0
            } else {
                byte
            };
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = 0u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        let crc = !crc;
        Ok(if crc == 0 { 1 } else { crc })
    }

    fn read_persisted_header_crc(bytes: &[u8]) -> AndromedaResult<u32> {
        let mut value = [0; Self::HEADER_CRC_LEN];
        value.copy_from_slice(
            bytes
                .get(Self::HEADER_CRC_OFFSET..Self::HEADER_CRC_OFFSET + Self::HEADER_CRC_LEN)
                .ok_or_else(|| storage_error("page image is too small for header CRC"))?,
        );
        Ok(u32::from_le_bytes(value))
    }

    fn write_persisted_header_crc(bytes: &mut [u8], crc: u32) -> AndromedaResult<()> {
        bytes
            .get_mut(Self::HEADER_CRC_OFFSET..Self::HEADER_CRC_OFFSET + Self::HEADER_CRC_LEN)
            .ok_or_else(|| storage_error("page image is too small for header CRC"))?
            .copy_from_slice(&crc.to_le_bytes());
        Ok(())
    }

    fn read_persisted_magic(bytes: &[u8]) -> AndromedaResult<u32> {
        let mut value = [0; 4];
        value.copy_from_slice(
            bytes
                .get(0..4)
                .ok_or_else(|| storage_error("page image is too small for page magic"))?,
        );
        Ok(u32::from_le_bytes(value))
    }

    fn validate_persisted_layout_marker(bytes: &[u8]) -> AndromedaResult<bool> {
        match Self::read_persisted_magic(bytes)? {
            0 if bytes.iter().all(|byte| *byte == 0) => Ok(false),
            0 => Err(storage_error(
                "Page integrity check failed: page layout marker is missing from non-empty page",
            )),
            PageHeader::MAGIC => Ok(true),
            _ => Err(storage_error(
                "Page integrity check failed: page layout marker is corrupted",
            )),
        }
    }

    pub(crate) fn stamp_page_integrity_if_enabled(
        &self,
        image: &mut PageImage,
    ) -> AndromedaResult<()> {
        match self.page_integrity_mode() {
            PageIntegrityMode::None => {
                let _ = image;
                Ok(())
            },
            PageIntegrityMode::HeaderCrc32 => {
                let Some(mut layout) = image.layout_contract() else {
                    return Err(DiskManagerError::PageLayoutInvalid {
                        reason: "HeaderCrc32 integrity mode requires page layout contract"
                            .to_string(),
                    }
                    .into());
                };
                if !Self::validate_persisted_layout_marker(image.as_bytes())? {
                    return Err(storage_error(
                        "HeaderCrc32 integrity mode requires persisted page layout bytes",
                    ));
                }

                let crc = Self::compute_integrity_crc32(image.as_bytes())?;
                layout.header.header_crc = crc;
                let mut bytes = image.as_bytes().to_vec();
                Self::write_persisted_header_crc(&mut bytes, crc)?;
                *image = PageImage::with_layout(layout, bytes).map_err(|e| {
                    DiskManagerError::PageLayoutInvalid {
                        reason: format!("failed to stamp page integrity CRC: {}", e.message()),
                    }
                })?;
                Ok(())
            },
        }
    }

    pub(crate) fn validate_page_integrity(&self, image: &PageImage) -> AndromedaResult<()> {
        match self.page_integrity_mode() {
            PageIntegrityMode::None => Ok(()),
            PageIntegrityMode::HeaderCrc32 => {
                if !Self::validate_persisted_layout_marker(image.as_bytes())? {
                    return Ok(());
                }
                let expected = Self::read_persisted_header_crc(image.as_bytes())?;
                if expected == 0 {
                    return Err(storage_error(
                        "Page integrity check failed: persisted header CRC is zero",
                    ));
                }
                let actual = Self::compute_integrity_crc32(image.as_bytes())?;
                if expected != actual {
                    return Err(DiskManagerError::PageCorrupted {
                        page_id: image.page_id().map_or(0, |page_id| page_id.get()),
                        reason: format!(
                            "Page integrity check failed: expected header CRC {expected:#010x}, got {actual:#010x}"
                        ),
                    }
                    .into());
                }
                Ok(())
            },
        }
    }
}

fn storage_error(message: impl Into<String>) -> andromeda_core::AndromedaError {
    DiskManagerError::PageCorrupted {
        page_id: 0,
        reason: message.into(),
    }
    .into()
}
