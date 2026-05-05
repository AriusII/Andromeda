use std::collections::BTreeMap;

use andromeda_core::AndromedaResult;

use crate::{Lsn, PageId, PageImage, PageLayoutContract, PageStore};

use super::{
    BufferFrame, BufferFrameId, BufferFrameState, BufferPoolConfig, BufferPoolError,
    ClockEvictionPolicy, DirtyTracker, PageGuard, PageGuardMut,
};

/// Minimal buffer-pool manager contract for fixed-capacity residency work.
///
/// Implementations must validate [`PageLayoutContract`] before admitting page
/// images and must preserve WAL-before-page-flush ordering before later flush
/// implementations make dirty contents durable.
pub trait BufferPoolManager {
    fn config(&self) -> &BufferPoolConfig;

    fn pin_page(&mut self, page_id: PageId) -> AndromedaResult<BufferFrameId>;

    fn unpin_page(
        &mut self,
        frame_id: BufferFrameId,
        dirty_lsn: Option<Lsn>,
    ) -> AndromedaResult<()>;

    fn flush_all_dirty(&mut self) -> AndromedaResult<()>;

    fn new_page(
        &mut self,
        layout_contract: PageLayoutContract,
    ) -> AndromedaResult<(PageId, BufferFrameId)>;
}

/// Fixed-capacity V0 buffer pool over a deterministic [`PageStore`].
///
/// Durable page identity, layout, image construction, and page-store allocation
/// remain owned by `page.rs` and [`PageStore`]. This type owns only transient
/// residency: frame table lookup, pins, dirty metadata, and Clock replacement.
#[derive(Debug)]
pub struct BufferPool<S: PageStore> {
    config: BufferPoolConfig,
    store: S,
    frames: Vec<BufferFrame>,
    page_table: BTreeMap<PageId, usize>,
    clock: ClockEvictionPolicy,
    dirty_tracker: DirtyTracker,
}

impl<S: PageStore> BufferPool<S> {
    pub fn new(config: BufferPoolConfig, store: S) -> AndromedaResult<Self> {
        config.validate()?;
        if store.page_size() != config.page_size() {
            return Err(BufferPoolError::PageSizeMismatch.into_andromeda_error());
        }

        let mut frames = Vec::with_capacity(config.frame_count());
        for index in 0..config.frame_count() {
            frames.push(BufferFrame::free(
                BufferFrameId::new(index + 1)?,
                config.page_size(),
            )?);
        }

        Ok(Self {
            config,
            store,
            frames,
            page_table: BTreeMap::new(),
            clock: ClockEvictionPolicy::new(),
            dirty_tracker: DirtyTracker::new(),
        })
    }

    pub fn into_page_store(self) -> S {
        self.store
    }

    pub const fn config_ref(&self) -> &BufferPoolConfig {
        &self.config
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn resident_page_count(&self) -> usize {
        self.page_table.len()
    }

    pub fn contains_resident_page(&self, page_id: PageId) -> bool {
        self.page_table.contains_key(&page_id)
    }

    pub fn resident_frame_id(&self, page_id: PageId) -> Option<BufferFrameId> {
        self.page_table
            .get(&page_id)
            .map(|index| self.frames[*index].id())
    }

    pub fn frame_page_id(&self, frame_id: BufferFrameId) -> AndromedaResult<Option<PageId>> {
        let index = self.frame_index(frame_id)?;
        Ok(self.frames[index].page_id())
    }

    pub fn pin_count(&self, frame_id: BufferFrameId) -> AndromedaResult<u32> {
        let index = self.frame_index(frame_id)?;
        Ok(self.frames[index].pin_count())
    }

    pub fn is_dirty(&self, page_id: PageId) -> AndromedaResult<bool> {
        self.dirty_tracker.is_dirty(page_id)
    }

    pub fn dirty_tracker(&self) -> &DirtyTracker {
        &self.dirty_tracker
    }

    pub fn fetch_page(&mut self, page_id: PageId) -> AndromedaResult<PageGuard<'_>> {
        let index = self.ensure_resident(page_id)?;
        self.frames[index].pin_guard()
    }

    pub fn fetch_page_mut(&mut self, page_id: PageId) -> AndromedaResult<PageGuardMut<'_>> {
        let index = self.ensure_resident(page_id)?;
        let (frames, dirty_tracker) = (&mut self.frames, &mut self.dirty_tracker);
        PageGuardMut::new_with_dirty_tracker(&mut frames[index], dirty_tracker)
    }

    /// Allocate a page through [`PageStore`], admit it into a resident frame, and
    /// return a mutable pin guard for deterministic initialization.
    pub fn new_page(
        &mut self,
        layout_contract: PageLayoutContract,
    ) -> AndromedaResult<(PageId, PageGuardMut<'_>)> {
        let page_id = layout_contract.header.page_id;
        let index = self.allocate_resident_frame(layout_contract)?;
        let (frames, dirty_tracker) = (&mut self.frames, &mut self.dirty_tracker);
        Ok((
            page_id,
            PageGuardMut::new_with_dirty_tracker(&mut frames[index], dirty_tracker)?,
        ))
    }

    fn ensure_resident(&mut self, page_id: PageId) -> AndromedaResult<usize> {
        validate_page_id(page_id)?;
        if let Some(index) = self.page_table.get(&page_id).copied() {
            return Ok(index);
        }

        let image = self
            .store
            .read_page(page_id)?
            .ok_or_else(|| BufferPoolError::PageNotFound { page_id: page_id.get() }.into_andromeda_error())?;
        if image.page_id() != Some(page_id) {
            return Err(BufferPoolError::InvalidPageImage.into_andromeda_error());
        }
        self.admit_image(image)
    }

    fn allocate_resident_frame(
        &mut self,
        layout_contract: PageLayoutContract,
    ) -> AndromedaResult<usize> {
        layout_contract
            .validate()
            .map_err(|_| BufferPoolError::InvalidPageLayout.into_andromeda_error())?;
        if layout_contract.header.page_size != self.config.page_size() {
            return Err(BufferPoolError::PageSizeMismatch.into_andromeda_error());
        }
        let page_id = layout_contract.header.page_id;
        validate_page_id(page_id)?;
        if self.page_table.contains_key(&page_id) {
            return Err(BufferPoolError::PageTableConflict { page_id: page_id.get() }
                .into_andromeda_error());
        }

        let image = self
            .store
            .allocate_page(layout_contract, layout_contract.header.page_lsn)?;
        self.admit_image(image)
    }

    fn admit_image(&mut self, image: PageImage) -> AndromedaResult<usize> {
        let page_id = validate_image_for_pool(&image, self.config.page_size())?;
        if self.page_table.contains_key(&page_id) {
            return Err(
                BufferPoolError::PageTableConflict { page_id: page_id.get() }
                    .into_andromeda_error(),
            );
        }

        let index = self.reusable_frame_index()?;
        let frame_id = self.frames[index].id();
        self.frames[index] = BufferFrame::with_image(frame_id, image)?;
        self.page_table.insert(page_id, index);
        Ok(index)
    }

    fn reusable_frame_index(&mut self) -> AndromedaResult<usize> {
        if let Some(index) = self
            .frames
            .iter()
            .position(|frame| frame.state() == BufferFrameState::Free)
        {
            return Ok(index);
        }

        let victim = self.clock.select_victim(&mut self.frames)?;
        let index = victim.frame_index();
        let evicted_page_id = self.frames[index]
            .page_id()
            .ok_or_else(|| BufferPoolError::InvalidFrameState.into_andromeda_error())?;

        self.frames[index].begin_eviction()?;
        self.page_table.remove(&evicted_page_id);
        self.dirty_tracker.mark_clean(evicted_page_id)?;
        self.frames[index].finish_eviction()?;
        Ok(index)
    }

    fn frame_index(&self, frame_id: BufferFrameId) -> AndromedaResult<usize> {
        frame_id.validate()?;
        let index = frame_id.zero_based_index();
        if index >= self.frames.len() {
            return Err(BufferPoolError::InvalidFrameId {
                frame_id: frame_id.get(),
            }
            .into_andromeda_error());
        }
        Ok(index)
    }
}

impl<S: PageStore> BufferPoolManager for BufferPool<S> {
    fn config(&self) -> &BufferPoolConfig {
        &self.config
    }

    fn pin_page(&mut self, page_id: PageId) -> AndromedaResult<BufferFrameId> {
        let index = self.ensure_resident(page_id)?;
        self.frames[index].pin()?;
        Ok(self.frames[index].id())
    }

    fn unpin_page(
        &mut self,
        frame_id: BufferFrameId,
        dirty_lsn: Option<Lsn>,
    ) -> AndromedaResult<()> {
        let index = self.frame_index(frame_id)?;
        if let Some(dirty_lsn) = dirty_lsn {
            self.frames[index].mark_dirty(dirty_lsn)?;
            let page_id = self.frames[index]
                .page_id()
                .ok_or_else(|| BufferPoolError::InvalidFrameState.into_andromeda_error())?;
            self.dirty_tracker.mark_dirty(page_id, dirty_lsn)?;
        }
        self.frames[index].unpin()
    }

    fn flush_all_dirty(&mut self) -> AndromedaResult<()> {
        let candidates = self.dirty_tracker.flush_candidates();
        for candidate in candidates {
            let page_id = candidate.page_id();
            let index = *self
                .page_table
                .get(&page_id)
                .ok_or_else(|| BufferPoolError::InvalidFrameState.into_andromeda_error())?;
            if self.frames[index].pin_count() != 0 {
                return Err(BufferPoolError::AllFramesPinned.into_andromeda_error());
            }

            let image = self.frames[index]
                .image()
                .cloned()
                .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
            self.frames[index].begin_flush()?;
            self.store.write_page(image, candidate.first_dirty_lsn())?;
            self.frames[index].finish_flush(candidate.first_dirty_lsn())?;
            self.dirty_tracker.mark_clean(page_id)?;
        }
        Ok(())
    }

    fn new_page(
        &mut self,
        layout_contract: PageLayoutContract,
    ) -> AndromedaResult<(PageId, BufferFrameId)> {
        let page_id = layout_contract.header.page_id;
        let index = self.allocate_resident_frame(layout_contract)?;
        self.frames[index].pin()?;
        Ok((page_id, self.frames[index].id()))
    }
}

fn validate_page_id(page_id: PageId) -> AndromedaResult<()> {
    if page_id.is_zero() {
        return Err(BufferPoolError::InvalidPageId.into_andromeda_error());
    }
    Ok(())
}

fn validate_image_for_pool(image: &PageImage, page_size: crate::PageSize) -> AndromedaResult<PageId> {
    if image.page_size() != page_size {
        return Err(BufferPoolError::PageSizeMismatch.into_andromeda_error());
    }
    let layout_contract = image
        .layout_contract()
        .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
    layout_contract
        .validate()
        .map_err(|_| BufferPoolError::InvalidPageLayout.into_andromeda_error())?;
    let page_id = layout_contract.header.page_id;
    validate_page_id(page_id)?;
    Ok(page_id)
}

#[cfg(test)]
mod tests {
    use andromeda_core::AndromedaErrorKind;

    use super::*;
    use crate::{
        AllocationId, InMemoryPageStore, ObjectId, PageFlags, PageHeader, PageSize, PageTrailer,
        PageType,
    };

    fn valid_contract(page_id: PageId, page_lsn: Lsn) -> PageLayoutContract {
        PageLayoutContract {
            header: PageHeader {
                magic: PageHeader::MAGIC,
                format_version: PageHeader::FORMAT_VERSION_V0,
                page_size: PageSize::KiB16,
                page_type: PageType::FixedRow,
                page_id,
                object_id: ObjectId::new(2),
                allocation_id: AllocationId::new(3),
                page_lsn,
                page_epoch: 1,
                previous_page_id: None,
                next_page_id: None,
                header_len: PageHeader::MIN_HEADER_LEN_V0,
                payload_offset: 128,
                payload_len: 512,
                free_start: 256,
                free_end: 512,
                free_bytes: 256,
                slot_count: 1,
                row_count: 1,
                flags: PageFlags::NONE,
                header_crc: 5,
            },
            trailer: PageTrailer {
                payload_crc64: 6,
                page_hash: [7; 32],
                torn_write_guard: 8,
            },
        }
    }

    fn pool(frame_count: usize) -> BufferPool<InMemoryPageStore> {
        BufferPool::new(
            BufferPoolConfig::new(frame_count, PageSize::KiB16).expect("valid config"),
            InMemoryPageStore::new(PageSize::KiB16),
        )
        .expect("valid pool")
    }

    #[test]
    fn fetch_existing_page_loads_and_pins() {
        let mut store = InMemoryPageStore::new(PageSize::KiB16);
        store
            .allocate_page(valid_contract(PageId::new(10), Lsn::new(10)), Lsn::new(10))
            .expect("allocated page");
        let mut pool = BufferPool::new(
            BufferPoolConfig::new(2, PageSize::KiB16).expect("config"),
            store,
        )
        .expect("pool");

        let frame_id = {
            let guard = pool.fetch_page(PageId::new(10)).expect("fetch resident");
            assert_eq!(guard.page_id(), Some(PageId::new(10)));
            assert_eq!(guard.pin_count(), 1);
            guard.frame_id()
        };

        assert_eq!(pool.pin_count(frame_id).expect("pin count"), 0);
        assert_eq!(pool.resident_frame_id(PageId::new(10)), Some(frame_id));
    }

    #[test]
    fn repeated_fetch_hits_same_frame_and_guard_drop_unpins() {
        let mut pool = pool(1);
        {
            let (_page_id, guard) = pool
                .new_page(valid_contract(PageId::new(11), Lsn::new(11)))
                .expect("new page");
            assert_eq!(guard.pin_count(), 1);
        }

        let first_frame = pool
            .resident_frame_id(PageId::new(11))
            .expect("resident frame");
        {
            let guard = pool.fetch_page(PageId::new(11)).expect("fetch hit");
            assert_eq!(guard.frame_id(), first_frame);
            assert_eq!(guard.pin_count(), 1);
        }
        assert_eq!(pool.pin_count(first_frame).expect("pin count"), 0);
    }

    #[test]
    fn new_page_allocates_resident_frame() {
        let mut pool = pool(2);
        let (page_id, frame_id) = {
            let (page_id, guard) = pool
                .new_page(valid_contract(PageId::new(12), Lsn::new(12)))
                .expect("new page");
            (page_id, guard.frame_id())
        };

        assert_eq!(page_id, PageId::new(12));
        assert_eq!(pool.resident_frame_id(PageId::new(12)), Some(frame_id));
        assert_eq!(pool.pin_count(frame_id).expect("pin count"), 0);
    }

    #[test]
    fn full_pool_evicts_clean_unpinned_page_through_clock() {
        let mut pool = pool(1);
        {
            let _ = pool
                .new_page(valid_contract(PageId::new(20), Lsn::new(20)))
                .expect("first page");
        }
        let first_frame = pool
            .resident_frame_id(PageId::new(20))
            .expect("first resident");

        {
            let (_page_id, guard) = pool
                .new_page(valid_contract(PageId::new(21), Lsn::new(21)))
                .expect("second page evicts first");
            assert_eq!(guard.frame_id(), first_frame);
        }

        assert!(!pool.contains_resident_page(PageId::new(20)));
        assert_eq!(pool.resident_frame_id(PageId::new(21)), Some(first_frame));
        assert_eq!(
            pool.frame_page_id(first_frame).expect("frame page"),
            Some(PageId::new(21))
        );
    }

    #[test]
    fn pinned_exhaustion_returns_explicit_storage_error() {
        let mut pool = pool(1);
        let _pinned = pool
            .new_page(valid_contract(PageId::new(30), Lsn::new(30)))
            .expect("pinned first");

        let error = pool
            .new_page(valid_contract(PageId::new(31), Lsn::new(31)))
            .expect_err("no victim while first page is pinned");

        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
        assert!(error.message().contains("every resident frame is pinned"));
    }

    #[test]
    fn dirty_exhaustion_returns_explicit_storage_error_and_tracker_is_synced() {
        let mut pool = pool(1);
        {
            let (_page_id, mut guard) = pool
                .new_page(valid_contract(PageId::new(40), Lsn::new(40)))
                .expect("new page");
            guard.mark_dirty(Lsn::new(40)).expect("dirty mark");
        }

        assert!(pool.is_dirty(PageId::new(40)).expect("dirty tracked"));
        let error = pool
            .new_page(valid_contract(PageId::new(41), Lsn::new(41)))
            .expect_err("dirty page cannot be evicted");

        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
        assert!(error.message().contains("no clean unpinned resident frame"));
        assert!(pool.contains_resident_page(PageId::new(40)));
    }

    #[test]
    fn frame_table_updates_on_eviction() {
        let mut pool = pool(2);
        {
            let _ = pool
                .new_page(valid_contract(PageId::new(50), Lsn::new(50)))
                .expect("first page");
        }
        {
            let _ = pool
                .new_page(valid_contract(PageId::new(51), Lsn::new(51)))
                .expect("second page");
        }
        let evicted_frame = pool
            .resident_frame_id(PageId::new(50))
            .expect("first resident");

        {
            let _ = pool
                .new_page(valid_contract(PageId::new(52), Lsn::new(52)))
                .expect("third page evicts first by Clock order");
        }

        assert!(!pool.contains_resident_page(PageId::new(50)));
        assert_eq!(pool.resident_frame_id(PageId::new(52)), Some(evicted_frame));
        assert_eq!(pool.resident_page_count(), 2);
    }
}
