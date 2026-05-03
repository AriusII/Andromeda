use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageId(u64);

impl PageId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(u64);

impl ObjectId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AllocationId(u64);

impl AllocationId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

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
pub struct PageFlags(u16);

impl PageFlags {
    pub const NONE: Self = Self(0);
    pub const HAS_PREVIOUS: Self = Self(1 << 0);
    pub const HAS_NEXT: Self = Self(1 << 1);
    pub const COLD_IMMUTABLE_IMAGE: Self = Self(1 << 2);

    const KNOWN_MASK: u16 = Self::HAS_PREVIOUS.0 | Self::HAS_NEXT.0 | Self::COLD_IMMUTABLE_IMAGE.0;

    pub const fn new(bits: u16) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn contains(self, flag: Self) -> bool {
        (self.0 & flag.0) == flag.0
    }

    pub const fn with(self, flag: Self) -> Self {
        Self(self.0 | flag.0)
    }

    pub const fn has_only_known_bits(self) -> bool {
        (self.0 & !Self::KNOWN_MASK) == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageHeader {
    pub magic: u32,
    pub format_version: u16,
    pub page_size: PageSize,
    pub page_type: PageType,
    pub page_id: PageId,
    pub object_id: ObjectId,
    pub allocation_id: AllocationId,
    pub page_lsn: Lsn,
    pub page_epoch: u64,
    pub previous_page_id: Option<PageId>,
    pub next_page_id: Option<PageId>,
    pub header_len: u16,
    pub payload_offset: u32,
    pub payload_len: u32,
    pub free_start: u32,
    pub free_end: u32,
    pub free_bytes: u32,
    pub slot_count: u16,
    pub row_count: u32,
    pub flags: PageFlags,
    pub header_crc: u32,
}

impl PageHeader {
    pub const MAGIC: u32 = 0x414E4452;
    pub const FORMAT_VERSION_V0: u16 = 1;
    pub const MIN_HEADER_LEN_V0: u16 = 96;

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.magic != Self::MAGIC {
            return Err(storage_error("page header magic mismatch"));
        }
        if self.format_version != Self::FORMAT_VERSION_V0 {
            return Err(storage_error("unsupported page format version"));
        }
        if self.page_id.is_zero() {
            return Err(storage_error("page id must not be zero"));
        }
        if self.page_type != PageType::Free
            && (self.object_id.is_zero() || self.allocation_id.is_zero())
        {
            return Err(storage_error(
                "non-free page object/allocation identity must not be zero",
            ));
        }
        if self.page_type == PageType::Free && (self.slot_count != 0 || self.row_count != 0) {
            return Err(storage_error("free page must not advertise rows or slots"));
        }
        if self.page_lsn.is_zero() {
            return Err(storage_error("page LSN must not be zero"));
        }
        if self.page_epoch == 0 {
            return Err(storage_error("page epoch must not be zero"));
        }
        if matches!(self.previous_page_id, Some(previous) if previous == self.page_id)
            || matches!(self.next_page_id, Some(next) if next == self.page_id)
        {
            return Err(storage_error("page links must not point to self"));
        }
        if matches!((self.previous_page_id, self.next_page_id), (Some(previous), Some(next)) if previous == next)
        {
            return Err(storage_error("page previous and next links must differ"));
        }
        if self.previous_page_id.is_some() != self.flags.contains(PageFlags::HAS_PREVIOUS) {
            return Err(storage_error("page previous-link flag mismatch"));
        }
        if self.next_page_id.is_some() != self.flags.contains(PageFlags::HAS_NEXT) {
            return Err(storage_error("page next-link flag mismatch"));
        }
        if !self.flags.has_only_known_bits() {
            return Err(storage_error("page flags contain unknown bits"));
        }
        if self.header_len < Self::MIN_HEADER_LEN_V0 {
            return Err(storage_error("page header length is below V0 minimum"));
        }

        let page_bytes = self.page_size.bytes();
        if u32::from(self.header_len) >= page_bytes {
            return Err(storage_error("page header length exceeds page size"));
        }
        if self.payload_offset < u32::from(self.header_len) {
            return Err(storage_error("page payload offset overlaps header"));
        }

        let payload_end = checked_add(self.payload_offset, self.payload_len, "page payload end")?;
        let max_payload_end = page_bytes
            .checked_sub(PageTrailer::V0_LEN)
            .ok_or_else(|| storage_error("page trailer exceeds page size"))?;
        if payload_end > max_payload_end {
            return Err(storage_error(
                "page payload overlaps trailer or exceeds page size",
            ));
        }
        if self.free_start < self.payload_offset
            || self.free_start > self.free_end
            || self.free_end > payload_end
        {
            return Err(storage_error("page free-space offsets are invalid"));
        }
        if self.free_bytes != self.free_end - self.free_start {
            return Err(storage_error("page free byte count does not match offsets"));
        }
        if self.row_count > u32::from(self.slot_count) {
            return Err(storage_error("page row count exceeds slot count"));
        }
        if self.header_crc == 0 {
            return Err(storage_error("page header CRC must not be zero"));
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
    pub const V0_LEN: u32 = 8 + 32 + 8;

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.payload_crc64 == 0 || self.torn_write_guard == 0 {
            return Err(storage_error("page trailer checks must not be zero"));
        }
        if self.page_hash == [0; 32] {
            return Err(storage_error("page payload hash must not be zero"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLayoutContract {
    pub header: PageHeader,
    pub trailer: PageTrailer,
}

impl PageLayoutContract {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.header.validate()?;
        self.trailer.validate()?;
        if self.trailer.torn_write_guard == self.header.page_id.get() {
            return Err(storage_error(
                "page torn-write guard must not be only the page id",
            ));
        }
        Ok(())
    }
}

fn checked_add(lhs: u32, rhs: u32, context: &'static str) -> AndromedaResult<u32> {
    lhs.checked_add(rhs)
        .ok_or_else(|| storage_error(format!("{context} overflows u32")))
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_header() -> PageHeader {
        PageHeader {
            magic: PageHeader::MAGIC,
            format_version: PageHeader::FORMAT_VERSION_V0,
            page_size: PageSize::KiB16,
            page_type: PageType::FixedRow,
            page_id: PageId::new(101),
            object_id: ObjectId::new(7),
            allocation_id: AllocationId::new(9),
            page_lsn: Lsn::new(10),
            page_epoch: 1,
            previous_page_id: Some(PageId::new(100)),
            next_page_id: Some(PageId::new(102)),
            header_len: PageHeader::MIN_HEADER_LEN_V0,
            payload_offset: 128,
            payload_len: 1024,
            free_start: 512,
            free_end: 768,
            free_bytes: 256,
            slot_count: 2,
            row_count: 2,
            flags: PageFlags::HAS_PREVIOUS.with(PageFlags::HAS_NEXT),
            header_crc: 7,
        }
    }

    fn valid_trailer() -> PageTrailer {
        PageTrailer {
            payload_crc64: 11,
            page_hash: [3; 32],
            torn_write_guard: 13,
        }
    }

    #[test]
    fn page_header_and_trailer_have_test_vectors() {
        assert!(valid_header().validate().is_ok());
        assert!(valid_trailer().validate().is_ok());
    }

    #[test]
    fn page_layout_contract_rejects_invalid_offsets_and_rows() {
        let mut header = valid_header();
        header.free_end = header.payload_offset + header.payload_len + 1;
        assert_eq!(
            header.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );

        let mut header = valid_header();
        header.row_count = u32::from(header.slot_count) + 1;
        assert_eq!(
            header.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn page_layout_contract_rejects_identity_and_guard_failures() {
        let mut header = valid_header();
        header.page_id = PageId::new(0);
        assert_eq!(
            header.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );

        let contract = PageLayoutContract {
            header: valid_header(),
            trailer: PageTrailer {
                torn_write_guard: valid_header().page_id.get(),
                ..valid_trailer()
            },
        };
        assert_eq!(
            contract.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
