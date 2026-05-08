use std::collections::BTreeMap;

use andromeda_error::AndromedaResult;

use andromeda_wal::Lsn;

use super::error::storage_error;
use super::identity::PageId;
use super::image::PageImage;
use super::layout::{PageLayoutContract, PageSize};

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
        super::validate_wal_durability_before_page_flush(page_lsn, durable_lsn).map_err(|_| {
            storage_error(
                "WAL-before-page-flush precondition failed: durable WAL LSN is behind page LSN",
            )
        })
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
