use std::collections::BTreeMap;

use andromeda_error::AndromedaResult;
use andromeda_storage_page::{Lsn, PageId, PageImage, PageLayoutContract, PageStore};

use crate::metrics::BufferPoolMetricsAccumulator;
use crate::pool_error::map_core_error;
use crate::{
    BufferFrame, BufferFrameId, BufferFrameState, BufferPoolConfig, BufferPoolError,
    BufferPoolMetrics, ClockEvictionPolicy, DirtyTracker, PageGuard, PageGuardMut,
};

mod flush;
mod validation;

use self::validation::{validate_image_for_pool, validate_page_id};

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
    metrics: BufferPoolMetricsAccumulator,
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
                BufferFrameId::new(index + 1).map_err(map_core_error)?,
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
            metrics: BufferPoolMetricsAccumulator::new(),
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

    pub fn metrics(&self) -> BufferPoolMetrics {
        self.metrics.snapshot(
            self.config.frame_count(),
            self.page_table.len(),
            self.dirty_tracker.len(),
        )
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
            self.metrics.record_hit();
            return Ok(index);
        }
        self.metrics.record_miss();

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

        let victim = match self.clock.select_victim(&mut self.frames) {
            Ok(victim) => victim,
            Err(error) => {
                self.metrics.record_eviction_stall();
                return Err(map_core_error(error));
            },
        };
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
        frame_id.validate().map_err(map_core_error)?;
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
