use std::collections::BTreeMap;

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

    pub const fn bytes_usize(self) -> usize {
        self.bytes() as usize
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

/// Full durable page byte image.
///
/// A `PageImage` is always exactly one canonical Andromeda page: either 16 KiB
/// or 32 KiB as selected by [`PageSize`]. The raw constructor intentionally
/// exposes no durable identity because binary header parsing is owned by the
/// page-layout contract. Callers that need `page_id` or `page_lsn` must attach a
/// validated [`PageLayoutContract`] with [`PageImage::with_layout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageImage {
    page_size: PageSize,
    bytes: Box<[u8]>,
    layout_contract: Option<PageLayoutContract>,
}

impl PageImage {
    pub fn new(page_size: PageSize, bytes: Vec<u8>) -> AndromedaResult<Self> {
        Self::validate_len(page_size, bytes.len())?;
        Ok(Self {
            page_size,
            bytes: bytes.into_boxed_slice(),
            layout_contract: None,
        })
    }

    pub fn with_layout(
        layout_contract: PageLayoutContract,
        bytes: Vec<u8>,
    ) -> AndromedaResult<Self> {
        layout_contract.validate()?;
        let page_size = layout_contract.header.page_size;
        Self::validate_len(page_size, bytes.len())?;
        Ok(Self {
            page_size,
            bytes: bytes.into_boxed_slice(),
            layout_contract: Some(layout_contract),
        })
    }

    pub fn zeroed_with_layout(layout_contract: PageLayoutContract) -> AndromedaResult<Self> {
        layout_contract.validate()?;
        let bytes = vec![0; layout_contract.header.page_size.bytes_usize()];
        Self::with_layout(layout_contract, bytes)
    }

    pub fn page_size(&self) -> PageSize {
        self.page_size
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes.into_vec()
    }

    pub fn layout_contract(&self) -> Option<PageLayoutContract> {
        self.layout_contract
    }

    pub fn page_id(&self) -> Option<PageId> {
        self.layout_contract.map(|contract| contract.header.page_id)
    }

    pub fn page_lsn(&self) -> Option<Lsn> {
        self.layout_contract
            .map(|contract| contract.header.page_lsn)
    }

    fn validate_len(page_size: PageSize, len: usize) -> AndromedaResult<()> {
        let expected = page_size.bytes_usize();
        if len != expected {
            return Err(storage_error(format!(
                "page image length {len} does not match canonical page size {expected}"
            )));
        }
        Ok(())
    }
}

/// Deterministic page-store abstraction for buffer-pool foundation tests.
///
/// Implementations are responsible for validating page layout contracts before
/// admitting images. `write_page` and `allocate_page` model the
/// WAL-before-page-flush precondition by requiring the caller-provided durable
/// WAL LSN to be greater than or equal to the image page LSN. Real disk IO,
/// fsync policy, and WAL manager integration are deliberately deferred.
pub trait PageStore {
    fn page_size(&self) -> PageSize;

    fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>>;

    fn write_page(&mut self, image: PageImage, durable_lsn: Lsn) -> AndromedaResult<()>;

    fn allocate_page(
        &mut self,
        layout_contract: PageLayoutContract,
        durable_lsn: Lsn,
    ) -> AndromedaResult<PageImage>;
}

/// Deterministic in-memory page store for unit and future buffer-pool tests.
///
/// This store is intentionally not a disk-IO implementation. It keeps pages in a
/// `BTreeMap` so page order is stable across runs and rejects writes for pages
/// that have not first been allocated.
#[derive(Debug, Clone)]
pub struct InMemoryPageStore {
    page_size: PageSize,
    pages: BTreeMap<PageId, PageImage>,
}

impl InMemoryPageStore {
    pub fn new(page_size: PageSize) -> Self {
        Self {
            page_size,
            pages: BTreeMap::new(),
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn contains_page(&self, page_id: PageId) -> bool {
        self.pages.contains_key(&page_id)
    }

    fn validate_page_id(page_id: PageId) -> AndromedaResult<()> {
        if page_id.is_zero() {
            return Err(storage_error("page store page id must not be zero"));
        }
        Ok(())
    }

    fn validate_image_for_store(&self, image: &PageImage) -> AndromedaResult<PageId> {
        if image.page_size() != self.page_size {
            return Err(storage_error(
                "page image size does not match page store size",
            ));
        }
        let layout_contract = image
            .layout_contract()
            .ok_or_else(|| storage_error("page image must include a validated layout contract"))?;
        layout_contract.validate()?;
        let page_id = layout_contract.header.page_id;
        Self::validate_page_id(page_id)?;
        Ok(page_id)
    }

    fn validate_wal_before_page_flush(image: &PageImage, durable_lsn: Lsn) -> AndromedaResult<()> {
        let page_lsn = image
            .page_lsn()
            .ok_or_else(|| storage_error("page image must expose page LSN through its layout"))?;
        if durable_lsn < page_lsn {
            return Err(storage_error(
                "WAL-before-page-flush precondition failed: durable WAL LSN is behind page LSN",
            ));
        }
        Ok(())
    }
}

impl PageStore for InMemoryPageStore {
    fn page_size(&self) -> PageSize {
        self.page_size
    }

    fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>> {
        Self::validate_page_id(page_id)?;
        Ok(self.pages.get(&page_id).cloned())
    }

    fn write_page(&mut self, image: PageImage, durable_lsn: Lsn) -> AndromedaResult<()> {
        let page_id = self.validate_image_for_store(&image)?;
        Self::validate_wal_before_page_flush(&image, durable_lsn)?;
        if !self.pages.contains_key(&page_id) {
            return Err(storage_error("page store write requires prior allocation"));
        }
        self.pages.insert(page_id, image);
        Ok(())
    }

    fn allocate_page(
        &mut self,
        layout_contract: PageLayoutContract,
        durable_lsn: Lsn,
    ) -> AndromedaResult<PageImage> {
        if layout_contract.header.page_size != self.page_size {
            return Err(storage_error(
                "allocated page size does not match page store size",
            ));
        }
        let image = PageImage::zeroed_with_layout(layout_contract)?;
        let page_id = self.validate_image_for_store(&image)?;
        Self::validate_wal_before_page_flush(&image, durable_lsn)?;
        if self.pages.contains_key(&page_id) {
            return Err(storage_error(
                "page store allocation would overwrite existing page",
            ));
        }
        self.pages.insert(page_id, image.clone());
        Ok(image)
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

    fn valid_contract(page_id: PageId, page_size: PageSize, page_lsn: Lsn) -> PageLayoutContract {
        PageLayoutContract {
            header: PageHeader {
                page_id,
                page_size,
                page_lsn,
                ..valid_header()
            },
            trailer: valid_trailer(),
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

    #[test]
    fn page_store_page_image_accepts_exact_16k_and_32k_lengths() {
        let image_16k =
            PageImage::new(PageSize::KiB16, vec![1; 16 * 1024]).expect("16KiB image is canonical");
        assert_eq!(image_16k.page_size(), PageSize::KiB16);
        assert_eq!(image_16k.len(), 16 * 1024);
        assert_eq!(image_16k.page_id(), None);
        assert_eq!(image_16k.page_lsn(), None);

        let contract = valid_contract(PageId::new(202), PageSize::KiB32, Lsn::new(22));
        let image_32k = PageImage::with_layout(contract, vec![2; 32 * 1024])
            .expect("32KiB image with valid layout is canonical");
        assert_eq!(image_32k.page_size(), PageSize::KiB32);
        assert_eq!(image_32k.len(), 32 * 1024);
        assert_eq!(image_32k.page_id(), Some(PageId::new(202)));
        assert_eq!(image_32k.page_lsn(), Some(Lsn::new(22)));
        assert_eq!(image_32k.layout_contract(), Some(contract));
    }

    #[test]
    fn page_store_page_image_rejects_wrong_lengths_and_invalid_layout() {
        assert_eq!(
            PageImage::new(PageSize::KiB16, vec![0; (16 * 1024) - 1])
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
        assert_eq!(
            PageImage::new(PageSize::KiB32, vec![0; (32 * 1024) + 1])
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );

        let invalid_contract = valid_contract(PageId::new(0), PageSize::KiB16, Lsn::new(12));
        assert_eq!(
            PageImage::with_layout(invalid_contract, vec![0; 16 * 1024])
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );

        let mismatched_len_contract =
            valid_contract(PageId::new(203), PageSize::KiB32, Lsn::new(23));
        assert_eq!(
            PageImage::with_layout(mismatched_len_contract, vec![0; 16 * 1024])
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn page_store_in_memory_allocate_read_write_is_deterministic() {
        let mut store = InMemoryPageStore::new(PageSize::KiB16);
        let contract = valid_contract(PageId::new(301), PageSize::KiB16, Lsn::new(30));

        assert_eq!(store.page_size(), PageSize::KiB16);
        assert_eq!(store.page_count(), 0);
        assert_eq!(
            store.read_page(PageId::new(301)).expect("read succeeds"),
            None
        );

        let allocated = store
            .allocate_page(contract, Lsn::new(30))
            .expect("allocation respects WAL precondition");
        assert_eq!(allocated.page_id(), Some(PageId::new(301)));
        assert_eq!(allocated.page_lsn(), Some(Lsn::new(30)));
        assert_eq!(store.page_count(), 1);
        assert!(store.contains_page(PageId::new(301)));
        assert_eq!(
            store.read_page(PageId::new(301)).expect("read allocated"),
            Some(allocated.clone())
        );

        let updated_contract = valid_contract(PageId::new(301), PageSize::KiB16, Lsn::new(31));
        let updated = PageImage::with_layout(updated_contract, vec![9; 16 * 1024])
            .expect("updated full-page image");
        store
            .write_page(updated.clone(), Lsn::new(31))
            .expect("write respects WAL precondition");
        assert_eq!(
            store.read_page(PageId::new(301)).expect("read updated"),
            Some(updated)
        );
    }

    #[test]
    fn page_store_rejects_unallocated_writes_duplicate_allocations_and_size_mismatch() {
        let mut store = InMemoryPageStore::new(PageSize::KiB16);
        let first_contract = valid_contract(PageId::new(401), PageSize::KiB16, Lsn::new(40));
        store
            .allocate_page(first_contract, Lsn::new(40))
            .expect("initial allocation");

        assert_eq!(
            store
                .allocate_page(first_contract, Lsn::new(40))
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );

        let unallocated = PageImage::with_layout(
            valid_contract(PageId::new(402), PageSize::KiB16, Lsn::new(41)),
            vec![4; 16 * 1024],
        )
        .expect("unallocated page image");
        assert_eq!(
            store
                .write_page(unallocated, Lsn::new(41))
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );

        let wrong_size_contract = valid_contract(PageId::new(403), PageSize::KiB32, Lsn::new(42));
        assert_eq!(
            store
                .allocate_page(wrong_size_contract, Lsn::new(42))
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );

        assert_eq!(
            store.read_page(PageId::new(0)).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn page_store_enforces_wal_before_page_flush_precondition() {
        let mut store = InMemoryPageStore::new(PageSize::KiB16);
        let contract = valid_contract(PageId::new(501), PageSize::KiB16, Lsn::new(50));

        assert_eq!(
            store
                .allocate_page(contract, Lsn::new(49))
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
        assert_eq!(store.page_count(), 0);

        let allocated = store
            .allocate_page(contract, Lsn::new(50))
            .expect("allocation succeeds once WAL durable LSN reaches page LSN");
        assert_eq!(allocated.page_lsn(), Some(Lsn::new(50)));

        let updated = PageImage::with_layout(
            valid_contract(PageId::new(501), PageSize::KiB16, Lsn::new(51)),
            vec![5; 16 * 1024],
        )
        .expect("updated image");
        assert_eq!(
            store
                .write_page(updated.clone(), Lsn::new(50))
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );

        store
            .write_page(updated, Lsn::new(51))
            .expect("write succeeds once WAL is durable through page LSN");
    }
}
