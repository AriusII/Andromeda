use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    KiB16,
    KiB32,
}

impl PageSize {
    pub const fn bytes(self) -> u32 {
        match self {
            Self::KiB16 => 16 * 1024,
            Self::KiB32 => 32 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageType {
    FixedRow,
    HybridRow,
    Manifest,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageHeader {
    pub magic: u32,
    pub format_version: u16,
    pub page_type: PageType,
    pub page_lsn: Lsn,
    pub page_epoch: u64,
    pub slot_count: u16,
    pub row_count: u32,
    pub header_crc: u32,
}

impl PageHeader {
    pub const MAGIC: u32 = 0x414E4452;
    pub const FORMAT_VERSION_V0: u16 = 1;

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.magic != Self::MAGIC {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "page header magic mismatch",
            ));
        }

        if self.format_version != Self::FORMAT_VERSION_V0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "unsupported page format version",
            ));
        }

        if self.header_crc == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "page header CRC must not be zero",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTrailer {
    pub payload_crc64: u64,
    pub page_hash: [u8; 32],
    pub torn_write_guard: u64,
}

impl PageTrailer {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.payload_crc64 == 0 || self.torn_write_guard == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "page trailer checks must not be zero",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_header_and_trailer_have_test_vectors() {
        let header = PageHeader {
            magic: PageHeader::MAGIC,
            format_version: PageHeader::FORMAT_VERSION_V0,
            page_type: PageType::FixedRow,
            page_lsn: Lsn::new(10),
            page_epoch: 1,
            slot_count: 2,
            row_count: 2,
            header_crc: 7,
        };
        let trailer = PageTrailer {
            payload_crc64: 11,
            page_hash: [3; 32],
            torn_write_guard: 13,
        };

        assert!(header.validate().is_ok());
        assert!(trailer.validate().is_ok());
    }
}
