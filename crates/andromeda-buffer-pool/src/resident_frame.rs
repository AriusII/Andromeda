use andromeda_error::AndromedaResult;
use andromeda_storage_page::{Lsn, PageId, PageImage, PageLayoutContract, PageSize};

use crate::pool_error::map_core_error;
use crate::{BufferFrameCore, BufferFrameId, BufferFrameState, BufferPoolError, ClockFrame};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferFrame {
    core: BufferFrameCore,
    image: Option<PageImage>,
    page_size: PageSize,
}

impl BufferFrame {
    pub fn free(id: BufferFrameId, page_size: PageSize) -> AndromedaResult<Self> {
        let frame = Self {
            core: BufferFrameCore::free(id).map_err(map_core_error)?,
            image: None,
            page_size,
        };
        frame.validate()?;
        Ok(frame)
    }

    pub fn new(id: BufferFrameId, page_id: PageId, page_size: PageSize) -> AndromedaResult<Self> {
        if page_id.is_zero() {
            return Err(BufferPoolError::InvalidPageId.into_andromeda_error());
        }
        Self::free(id, page_size)
    }

    pub fn with_layout(
        id: BufferFrameId,
        layout_contract: PageLayoutContract,
    ) -> AndromedaResult<Self> {
        let image = PageImage::zeroed_with_layout(layout_contract)
            .map_err(|_| BufferPoolError::InvalidPageLayout.into_andromeda_error())?;
        Self::with_image(id, image)
    }

    pub fn with_image(id: BufferFrameId, image: PageImage) -> AndromedaResult<Self> {
        let layout_contract = image
            .layout_contract()
            .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
        layout_contract
            .validate()
            .map_err(|_| BufferPoolError::InvalidPageLayout.into_andromeda_error())?;
        let page_id = image
            .page_id()
            .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
        if page_id.is_zero() {
            return Err(BufferPoolError::InvalidPageId.into_andromeda_error());
        }
        let page_lsn = image
            .page_lsn()
            .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
        if page_lsn.is_zero() {
            return Err(BufferPoolError::InvalidPageImage.into_andromeda_error());
        }
        let frame = Self {
            core: BufferFrameCore::resident(id, page_id.get(), page_lsn).map_err(map_core_error)?,
            page_size: image.page_size(),
            image: Some(image),
        };
        frame.validate()?;
        Ok(frame)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.core.validate().map_err(map_core_error)?;
        match self.core.state() {
            BufferFrameState::Free => {
                if self.image.is_some() {
                    return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
                }
            },
            BufferFrameState::Resident
            | BufferFrameState::Flushing
            | BufferFrameState::Evicting => {
                self.validate_resident_metadata()?;
            },
        }
        Ok(())
    }

    fn validate_resident_metadata(&self) -> AndromedaResult<()> {
        let image = self
            .image
            .as_ref()
            .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
        if image.page_size() != self.page_size {
            return Err(BufferPoolError::InvalidPageImage.into_andromeda_error());
        }
        let page_id = self
            .page_id()
            .ok_or_else(|| BufferPoolError::InvalidPageId.into_andromeda_error())?;
        if page_id.is_zero() {
            return Err(BufferPoolError::InvalidPageId.into_andromeda_error());
        }
        let image_page_id = image
            .page_id()
            .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
        if image_page_id != page_id {
            return Err(BufferPoolError::InvalidPageImage.into_andromeda_error());
        }
        let page_lsn = self
            .page_lsn()
            .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
        if page_lsn.is_zero() {
            return Err(BufferPoolError::InvalidPageImage.into_andromeda_error());
        }
        let image_page_lsn = image
            .page_lsn()
            .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
        if image_page_lsn != page_lsn {
            return Err(BufferPoolError::InvalidPageImage.into_andromeda_error());
        }
        Ok(())
    }

    pub const fn id(&self) -> BufferFrameId {
        self.core.id()
    }

    pub const fn state(&self) -> BufferFrameState {
        self.core.state()
    }

    pub fn page_id(&self) -> Option<PageId> {
        self.core.page_id_value().map(PageId::new)
    }

    pub const fn page_size(&self) -> PageSize {
        self.page_size
    }

    pub const fn page_lsn(&self) -> Option<Lsn> {
        self.core.page_lsn()
    }

    pub const fn pin_count(&self) -> u32 {
        self.core.pin_count()
    }

    pub const fn is_dirty(&self) -> bool {
        self.core.is_dirty()
    }

    pub const fn first_dirty_lsn(&self) -> Option<Lsn> {
        self.core.first_dirty_lsn()
    }

    pub const fn last_dirty_lsn(&self) -> Option<Lsn> {
        self.core.last_dirty_lsn()
    }

    pub const fn dirty_lsn(&self) -> Option<Lsn> {
        self.core.first_dirty_lsn()
    }

    pub fn image(&self) -> Option<&PageImage> {
        self.image.as_ref()
    }

    pub(crate) fn image_mut(&mut self) -> Option<&mut PageImage> {
        self.image.as_mut()
    }

    pub fn layout_contract(&self) -> Option<PageLayoutContract> {
        self.image.as_ref().and_then(PageImage::layout_contract)
    }

    pub const fn clock_usage(&self) -> bool {
        self.core.clock_usage()
    }

    pub fn pin(&mut self) -> AndromedaResult<()> {
        self.core.pin().map_err(map_core_error)
    }

    pub fn pin_guard(&mut self) -> AndromedaResult<super::PageGuard<'_>> {
        super::PageGuard::new(self)
    }

    pub fn pin_guard_mut(&mut self) -> AndromedaResult<super::PageGuardMut<'_>> {
        super::PageGuardMut::new(self)
    }

    pub fn unpin(&mut self) -> AndromedaResult<()> {
        self.core.unpin().map_err(map_core_error)
    }

    pub fn mark_dirty(&mut self, dirty_lsn: Lsn) -> AndromedaResult<()> {
        self.core.mark_dirty(dirty_lsn).map_err(map_core_error)
    }

    pub fn mark_clean_after_flush(&mut self, flushed_lsn: Lsn) -> AndromedaResult<()> {
        self.core
            .mark_clean_after_flush(flushed_lsn)
            .map_err(map_core_error)
    }

    pub fn mark_clean(&mut self) {
        self.core.mark_clean();
    }

    pub fn set_clock_usage(&mut self) -> AndromedaResult<()> {
        self.core.set_clock_usage().map_err(map_core_error)
    }

    pub fn consume_clock_usage(&mut self) -> AndromedaResult<bool> {
        self.core.consume_clock_usage().map_err(map_core_error)
    }

    pub fn begin_flush(&mut self) -> AndromedaResult<()> {
        self.core.begin_flush().map_err(map_core_error)
    }

    pub(crate) fn abort_flush(&mut self) -> AndromedaResult<()> {
        self.core.abort_flush().map_err(map_core_error)
    }

    pub fn finish_flush(&mut self, flushed_lsn: Lsn) -> AndromedaResult<()> {
        self.core.finish_flush(flushed_lsn).map_err(map_core_error)
    }

    pub fn begin_eviction(&mut self) -> AndromedaResult<()> {
        self.core.begin_eviction().map_err(map_core_error)
    }

    pub fn finish_eviction(&mut self) -> AndromedaResult<()> {
        self.core.finish_eviction().map_err(map_core_error)?;
        self.image = None;
        Ok(())
    }
}

impl ClockFrame for BufferFrame {
    fn clock_frame_id(&self) -> BufferFrameId {
        self.id()
    }

    fn clock_state(&self) -> BufferFrameState {
        self.state()
    }

    fn clock_pin_count(&self) -> u32 {
        self.pin_count()
    }

    fn clock_is_dirty(&self) -> bool {
        self.is_dirty()
    }

    fn consume_clock_usage(&mut self) -> Result<bool, crate::BufferPoolCoreError> {
        self.core.consume_clock_usage()
    }
}
