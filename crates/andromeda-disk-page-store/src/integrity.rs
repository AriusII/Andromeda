use andromeda_error::AndromedaResult;
use andromeda_storage_page::{
    PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET, PAGE_CODEC_V1_HEADER_LEN, PageHeader, PageImage,
    header_integrity_crc32,
};

use crate::PageIntegrityMode;

use super::{DiskManagerError, FileDiskManager};

impl FileDiskManager {
    /// Offset of the outer `header_crc` field within a PageCodecV1 112-byte header.
    /// This aligns with PageCodecV1 bytes 100-103 (the `header_crc` field).
    pub(crate) const HEADER_CRC_OFFSET: usize = 100;
    const HEADER_CRC_LEN: usize = 4;

    pub const fn page_integrity_mode(&self) -> PageIntegrityMode {
        self.page_integrity_mode
    }

    fn compute_integrity_crc32(bytes: &[u8]) -> AndromedaResult<u32> {
        // The outer CRC zeroes both the `header_crc` field (bytes 100-103) AND the
        // `HeaderIntegrityCrc` field (bytes 104-107) during computation. This makes
        // both CRC fields independent — neither's computed value depends on the other —
        // so `stamp_page_integrity_if_enabled` can write HIC first, then write the outer
        // CRC without invalidating HIC or vice versa.
        let outer_range = Self::HEADER_CRC_OFFSET..Self::HEADER_CRC_OFFSET + Self::HEADER_CRC_LEN;
        let hic_range =
            PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET..PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET + 4;
        let mut crc = 0xFFFF_FFFFu32;
        for (offset, byte) in bytes.iter().copied().enumerate() {
            let byte = if outer_range.contains(&offset) || hic_range.contains(&offset) {
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
                // After writing the outer header_crc at bytes 100-103, the
                // PageCodecV1 HeaderIntegrityCrc at bytes 104-107 is stale.
                // Recompute it over the updated 112-byte header so that the
                // PageCodecV1 decode_header integrity check remains valid.
                if bytes.len() >= PAGE_CODEC_V1_HEADER_LEN {
                    let hic = header_integrity_crc32(&bytes[..PAGE_CODEC_V1_HEADER_LEN]);
                    let hic_start = PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET;
                    bytes[hic_start..hic_start + 4].copy_from_slice(&hic.to_le_bytes());
                }
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

fn storage_error(message: impl Into<String>) -> andromeda_error::AndromedaError {
    DiskManagerError::PageCorrupted {
        page_id: 0,
        reason: message.into(),
    }
    .into()
}
