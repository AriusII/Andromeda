use andromeda_core::AndromedaResult;

use crate::{Lsn, PageId, PageImage, PageLayoutContract, PageSize};

use super::BufferPoolError;

/// Transient identifier for a buffer-pool frame.
///
/// This is not a durable page identity and must not be persisted in page images,
/// WAL payloads, manifests, or cold segment metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferFrameId(usize);

impl BufferFrameId {
    pub fn new(value: usize) -> AndromedaResult<Self> {
        let id = Self(value);
        id.validate()?;
        Ok(id)
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.0 == 0 {
            return Err(BufferPoolError::InvalidFrameId { frame_id: self.0 }.into_andromeda_error());
        }
        Ok(())
    }

    pub const fn get(self) -> usize {
        self.0
    }

    pub const fn zero_based_index(self) -> usize {
        self.0 - 1
    }
}

/// Transient lifecycle state for a buffer frame.
///
/// The state is buffer-pool metadata only. It must never be serialized into page
/// images, WAL records, manifests, or cold segment metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferFrameState {
    Free,
    Resident,
    Flushing,
    Evicting,
}

/// Transient metadata and resident image for a buffer-pool frame.
///
/// Durable identity, layout, page size, and LSN metadata are imported from the
/// canonical page/LSN owners. The frame only adds transient pin/dirty/Clock and
/// lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferFrame {
    id: BufferFrameId,
    state: BufferFrameState,
    image: Option<PageImage>,
    page_id: Option<PageId>,
    page_size: PageSize,
    page_lsn: Option<Lsn>,
    pin_count: u32,
    is_dirty: bool,
    first_dirty_lsn: Option<Lsn>,
    clock_usage: bool,
}

impl BufferFrame {
    /// Construct a free frame with no resident page image.
    pub fn free(id: BufferFrameId, page_size: PageSize) -> AndromedaResult<Self> {
        let frame = Self {
            id,
            state: BufferFrameState::Free,
            image: None,
            page_id: None,
            page_size,
            page_lsn: None,
            pin_count: 0,
            is_dirty: false,
            first_dirty_lsn: None,
            clock_usage: false,
        };
        frame.validate()?;
        Ok(frame)
    }

    /// Compatibility constructor for an empty free frame.
    ///
    /// New residency code should prefer [`Self::free`] or [`Self::with_image`].
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
            id,
            state: BufferFrameState::Resident,
            page_size: image.page_size(),
            image: Some(image),
            page_id: Some(page_id),
            page_lsn: Some(page_lsn),
            pin_count: 0,
            is_dirty: false,
            first_dirty_lsn: None,
            clock_usage: true,
        };
        frame.validate()?;
        Ok(frame)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.id.validate()?;
        if self.is_dirty && self.first_dirty_lsn.map_or(true, Lsn::is_zero) {
            return Err(BufferPoolError::InvalidDirtyLsn.into_andromeda_error());
        }
        if !self.is_dirty && self.first_dirty_lsn.is_some() {
            return Err(BufferPoolError::InvalidDirtyLsn.into_andromeda_error());
        }

        match self.state {
            BufferFrameState::Free => {
                if self.image.is_some()
                    || self.page_id.is_some()
                    || self.page_lsn.is_some()
                    || self.pin_count != 0
                    || self.is_dirty
                    || self.first_dirty_lsn.is_some()
                {
                    return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
                }
                if self.clock_usage {
                    return Err(BufferPoolError::InvalidClockUsage.into_andromeda_error());
                }
            }
            BufferFrameState::Resident
            | BufferFrameState::Flushing
            | BufferFrameState::Evicting => {
                self.validate_resident_metadata()?;
                if matches!(self.state, BufferFrameState::Flushing) && self.pin_count != 0 {
                    return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
                }
                if matches!(self.state, BufferFrameState::Evicting)
                    && (self.pin_count != 0 || self.is_dirty)
                {
                    return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
                }
            }
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
            .page_id
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
            .page_lsn
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
        if let Some(first_dirty_lsn) = self.first_dirty_lsn {
            if first_dirty_lsn.is_zero() || first_dirty_lsn < page_lsn {
                return Err(BufferPoolError::InvalidDirtyLsn.into_andromeda_error());
            }
        }
        Ok(())
    }

    pub const fn id(&self) -> BufferFrameId {
        self.id
    }

    pub const fn state(&self) -> BufferFrameState {
        self.state
    }

    pub const fn page_id(&self) -> Option<PageId> {
        self.page_id
    }

    pub const fn page_size(&self) -> PageSize {
        self.page_size
    }

    pub const fn page_lsn(&self) -> Option<Lsn> {
        self.page_lsn
    }

    pub const fn pin_count(&self) -> u32 {
        self.pin_count
    }

    pub const fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub const fn first_dirty_lsn(&self) -> Option<Lsn> {
        self.first_dirty_lsn
    }

    pub const fn dirty_lsn(&self) -> Option<Lsn> {
        self.first_dirty_lsn
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
        self.clock_usage
    }

    pub fn pin(&mut self) -> AndromedaResult<()> {
        if self.state != BufferFrameState::Resident {
            return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
        }
        self.pin_count = self
            .pin_count
            .checked_add(1)
            .ok_or_else(|| BufferPoolError::InvalidPinCount.into_andromeda_error())?;
        self.set_clock_usage()?;
        Ok(())
    }

    pub fn pin_guard(&mut self) -> AndromedaResult<super::PageGuard<'_>> {
        super::PageGuard::new(self)
    }

    pub fn pin_guard_mut(&mut self) -> AndromedaResult<super::PageGuardMut<'_>> {
        super::PageGuardMut::new(self)
    }

    pub fn unpin(&mut self) -> AndromedaResult<()> {
        if self.state != BufferFrameState::Resident {
            return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
        }
        self.pin_count = self
            .pin_count
            .checked_sub(1)
            .ok_or_else(|| BufferPoolError::InvalidPinCount.into_andromeda_error())?;
        Ok(())
    }

    pub fn mark_dirty(&mut self, dirty_lsn: Lsn) -> AndromedaResult<()> {
        if self.state != BufferFrameState::Resident {
            return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
        }
        let page_lsn = self
            .page_lsn
            .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
        if dirty_lsn.is_zero() || dirty_lsn < page_lsn {
            return Err(BufferPoolError::InvalidDirtyLsn.into_andromeda_error());
        }
        if let Some(first_dirty_lsn) = self.first_dirty_lsn {
            if dirty_lsn < first_dirty_lsn {
                return Err(BufferPoolError::InvalidDirtyLsn.into_andromeda_error());
            }
        } else {
            self.first_dirty_lsn = Some(dirty_lsn);
        }
        self.is_dirty = true;
        self.set_clock_usage()?;
        Ok(())
    }

    pub fn mark_clean_after_flush(&mut self, flushed_lsn: Lsn) -> AndromedaResult<()> {
        if !matches!(
            self.state,
            BufferFrameState::Resident | BufferFrameState::Flushing
        ) {
            return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
        }
        let first_dirty_lsn = self
            .first_dirty_lsn
            .ok_or_else(|| BufferPoolError::InvalidDirtyLsn.into_andromeda_error())?;
        if flushed_lsn < first_dirty_lsn {
            return Err(BufferPoolError::InvalidDirtyLsn.into_andromeda_error());
        }
        self.is_dirty = false;
        self.first_dirty_lsn = None;
        self.state = BufferFrameState::Resident;
        self.validate()
    }

    pub fn mark_clean(&mut self) {
        self.is_dirty = false;
        self.first_dirty_lsn = None;
        if self.state == BufferFrameState::Flushing {
            self.state = BufferFrameState::Resident;
        }
    }

    pub fn set_clock_usage(&mut self) -> AndromedaResult<()> {
        if self.state == BufferFrameState::Free {
            return Err(BufferPoolError::InvalidClockUsage.into_andromeda_error());
        }
        self.clock_usage = true;
        Ok(())
    }

    pub fn consume_clock_usage(&mut self) -> AndromedaResult<bool> {
        if self.state == BufferFrameState::Free {
            return Err(BufferPoolError::InvalidClockUsage.into_andromeda_error());
        }
        let previous = self.clock_usage;
        self.clock_usage = false;
        Ok(previous)
    }

    pub fn begin_flush(&mut self) -> AndromedaResult<()> {
        if self.state != BufferFrameState::Resident || !self.is_dirty || self.pin_count != 0 {
            return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
        }
        self.state = BufferFrameState::Flushing;
        self.validate()
    }

    pub fn finish_flush(&mut self, flushed_lsn: Lsn) -> AndromedaResult<()> {
        if self.state != BufferFrameState::Flushing {
            return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
        }
        self.mark_clean_after_flush(flushed_lsn)
    }

    pub fn begin_eviction(&mut self) -> AndromedaResult<()> {
        if self.state != BufferFrameState::Resident || self.pin_count != 0 || self.is_dirty {
            return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
        }
        self.state = BufferFrameState::Evicting;
        self.validate()
    }

    pub fn finish_eviction(&mut self) -> AndromedaResult<()> {
        if self.state != BufferFrameState::Evicting {
            return Err(BufferPoolError::InvalidFrameState.into_andromeda_error());
        }
        self.image = None;
        self.page_id = None;
        self.page_lsn = None;
        self.pin_count = 0;
        self.is_dirty = false;
        self.first_dirty_lsn = None;
        self.clock_usage = false;
        self.state = BufferFrameState::Free;
        self.validate()
    }
}
