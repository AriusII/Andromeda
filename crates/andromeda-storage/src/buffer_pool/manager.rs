use std::collections::BTreeMap;

use andromeda_core::AndromedaResult;

use crate::{
    Lsn, PageId, PageImage, PageLayoutContract, PageStore,
    validate_wal_durability_before_page_flush,
};

use super::{
    BufferFrame, BufferFrameId, BufferFrameState, BufferPoolConfig, BufferPoolError,
    ClockEvictionPolicy, DirtyTracker, FlushAllDirtyResult, FlushBlockedFrame, FlushError,
    FlushStorageOperation, PageGuard, PageGuardMut, WalDurabilityObserver,
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

        let image = self.store.read_page(page_id)?.ok_or_else(|| {
            BufferPoolError::PageNotFound {
                page_id: page_id.get(),
            }
            .into_andromeda_error()
        })?;
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
            return Err(BufferPoolError::PageTableConflict {
                page_id: page_id.get(),
            }
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
            return Err(BufferPoolError::PageTableConflict {
                page_id: page_id.get(),
            }
            .into_andromeda_error());
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
        if !self.dirty_tracker.is_empty() {
            return Err(BufferPoolError::WalDurabilityRequired.into_andromeda_error());
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

impl<S: PageStore> BufferPool<S> {
    /// Flush dirty frames that have LSNs durable in the WAL.
    ///
    /// This is the primary flush gate: before a page is written to disk,
    /// the buffer pool verifies that the page's latest dirty LSN has been made
    /// durable in the WAL via the observer.
    ///
    /// Returns the count of pages successfully flushed.
    pub fn flush_dirty_frames(
        &mut self,
        observer: &dyn WalDurabilityObserver,
    ) -> AndromedaResult<usize> {
        let candidates = self.dirty_tracker.flush_candidates();
        let mut flushed = 0;

        for candidate in candidates {
            let page_id = candidate.page_id();
            let last_dirty_lsn = candidate.last_dirty_lsn();
            let durable_lsn = observer.max_durable_lsn();

            // Check if every dirty update represented by the page is durable in the WAL.
            if !observer.is_durable(last_dirty_lsn)
                || validate_wal_durability_before_page_flush(last_dirty_lsn, durable_lsn).is_err()
            {
                // Cannot flush yet: LSN not durable in WAL
                continue;
            }

            let index = *self
                .page_table
                .get(&page_id)
                .ok_or_else(|| BufferPoolError::InvalidFrameState.into_andromeda_error())?;

            // Frame must not be pinned before flush
            if self.frames[index].pin_count() != 0 {
                return Err(BufferPoolError::AllFramesPinned.into_andromeda_error());
            }

            // Perform the flush
            let image = self.frames[index]
                .image()
                .cloned()
                .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
            self.frames[index].begin_flush()?;
            if let Err(error) = self.store.write_page(image, durable_lsn) {
                self.frames[index].abort_flush()?;
                return Err(error);
            }
            self.frames[index].finish_flush(last_dirty_lsn)?;
            self.dirty_tracker.mark_clean(page_id)?;

            flushed += 1;
        }

        Ok(flushed)
    }

    /// Flush all eligible dirty frames with complete blocked-state reporting.
    ///
    /// This method performs a single-pass flush of all dirty pages. For each page:
    ///
    /// - If the page's latest dirty LSN is durable in the WAL and the frame is not
    ///   pinned, the page is flushed and added to the `flushed` count.
    /// - If the page's latest dirty LSN is not yet durable in the WAL, the page is
    ///   added to `blocked_by_wal_durability` with LSN information. The page remains
    ///   dirty and should be retried after WAL advances.
    /// - If the page encounters an error during flush (pinned, IO error, invalid state),
    ///   the error is collected in the `errors` list. The page remains dirty.
    ///
    /// This contract ensures that callers have explicit visibility into why each dirty
    /// frame was or was not flushed in a single pass.
    ///
    /// **Key contract properties:**
    /// - All eligible frames are flushed in one pass
    /// - No panics; all errors are collected and reported
    /// - Blocked frames are tracked explicitly with LSN information
    /// - Failed flush attempts preserve the frame in dirty state
    /// - Result invariant: each dirty frame appears in exactly one result category
    pub fn flush_all_dirty_with_report(
        &mut self,
        observer: &dyn WalDurabilityObserver,
    ) -> AndromedaResult<FlushAllDirtyResult> {
        let mut result = FlushAllDirtyResult::new();
        let candidates = self.dirty_tracker.flush_candidates();

        for candidate in candidates {
            let page_id = candidate.page_id();
            let first_dirty_lsn = candidate.first_dirty_lsn();
            let last_dirty_lsn = candidate.last_dirty_lsn();
            let durable_lsn = observer.max_durable_lsn();

            // Gate 1: Check if every dirty update represented by the page is durable in the WAL
            if !observer.is_durable(last_dirty_lsn)
                || validate_wal_durability_before_page_flush(last_dirty_lsn, durable_lsn).is_err()
            {
                // Frame is blocked: LSN not yet durable
                result
                    .blocked_by_wal_durability
                    .push(FlushBlockedFrame::new_with_last_dirty_lsn(
                        page_id,
                        first_dirty_lsn,
                        last_dirty_lsn,
                        durable_lsn,
                    ));
                continue;
            }

            // Gate 2: Ensure page is still resident
            let index = match self.page_table.get(&page_id) {
                Some(&idx) => idx,
                None => {
                    result.errors.push(FlushError::FrameNotResident { page_id });
                    continue;
                }
            };

            // Gate 3: Ensure frame is not pinned
            let pin_count = self.frames[index].pin_count();
            if pin_count != 0 {
                result
                    .errors
                    .push(FlushError::FramePinned { page_id, pin_count });
                continue;
            }

            // Gate 4: Ensure page image is valid
            let image = match self.frames[index].image().cloned() {
                Some(img) => img,
                None => {
                    result.errors.push(FlushError::InvalidPageImage { page_id });
                    continue;
                }
            };

            // Attempt flush: begin_flush, write, finish_flush
            if self.frames[index].begin_flush().is_err() {
                result
                    .errors
                    .push(FlushError::InvalidFrameState { page_id });
                continue;
            }

            if let Err(e) = self.store.write_page(image, durable_lsn) {
                if self.frames[index].abort_flush().is_err() {
                    result
                        .errors
                        .push(FlushError::InvalidFrameState { page_id });
                    continue;
                }
                result.errors.push(FlushError::storage(
                    page_id,
                    FlushStorageOperation::PageStoreWrite,
                    e.message(),
                ));
                continue;
            }

            if self.frames[index].finish_flush(last_dirty_lsn).is_err() {
                result
                    .errors
                    .push(FlushError::InvalidFrameState { page_id });
                continue;
            }

            // Mark clean only on successful flush
            if self.dirty_tracker.mark_clean(page_id).is_err() {
                result.errors.push(FlushError::storage(
                    page_id,
                    FlushStorageOperation::DirtyTrackerMarkClean,
                    format!("failed to mark page {} clean after flush", page_id.get()),
                ));
                continue;
            }

            result.flushed += 1;
        }

        Ok(result)
    }
}

fn validate_page_id(page_id: PageId) -> AndromedaResult<()> {
    if page_id.is_zero() {
        return Err(BufferPoolError::InvalidPageId.into_andromeda_error());
    }
    Ok(())
}

fn validate_image_for_pool(
    image: &PageImage,
    page_size: crate::PageSize,
) -> AndromedaResult<PageId> {
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
        let frame_id = {
            let (_page_id, guard) = pool
                .new_page(valid_contract(PageId::new(30), Lsn::new(30)))
                .expect("pinned first");
            guard.frame_id()
        };
        pool.frames[frame_id.zero_based_index()]
            .pin()
            .expect("pin resident frame");

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
