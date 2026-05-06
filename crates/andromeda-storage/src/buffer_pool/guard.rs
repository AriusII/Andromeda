use andromeda_core::AndromedaResult;

use crate::{Lsn, PageId, PageImage, PageLayoutContract};

use super::{BufferFrame, BufferFrameId, DirtyTracker};

/// Immutable RAII guard for an acquired buffer-frame pin.
///
/// The guard owns no durable page identity or layout metadata. It only holds the
/// transient pin against a resident [`BufferFrame`] and releases that pin exactly
/// once when dropped, unless it has already been explicitly consumed by
/// [`Self::unpin`].
#[derive(Debug)]
pub struct PageGuard<'a> {
    frame: &'a mut BufferFrame,
    pinned: bool,
}

impl<'a> PageGuard<'a> {
    pub(crate) fn new(frame: &'a mut BufferFrame) -> AndromedaResult<Self> {
        frame.pin()?;
        Ok(Self {
            frame,
            pinned: true,
        })
    }

    pub fn frame_id(&self) -> BufferFrameId {
        self.frame.id()
    }

    pub fn page_id(&self) -> Option<PageId> {
        self.frame.page_id()
    }

    pub fn image(&self) -> Option<&PageImage> {
        self.frame.image()
    }

    pub fn layout_contract(&self) -> Option<PageLayoutContract> {
        self.frame.layout_contract()
    }

    pub fn pin_count(&self) -> u32 {
        self.frame.pin_count()
    }

    pub fn unpin(mut self) -> AndromedaResult<()> {
        self.unpin_once()
    }

    fn unpin_once(&mut self) -> AndromedaResult<()> {
        if self.pinned {
            self.frame.unpin()?;
            self.pinned = false;
        }
        Ok(())
    }
}

impl Drop for PageGuard<'_> {
    fn drop(&mut self) {
        if self.pinned {
            let _ = self.frame.unpin();
            self.pinned = false;
        }
    }
}

/// Mutable RAII guard for an acquired buffer-frame pin.
///
/// Dirty state remains controlled by buffer-pool metadata: callers must mark the
/// frame dirty explicitly with a non-zero LSN using [`Self::mark_dirty`]. The
/// first dirty LSN is preserved by [`BufferFrame::mark_dirty`].
#[derive(Debug)]
pub struct PageGuardMut<'a> {
    frame: &'a mut BufferFrame,
    dirty_tracker: Option<&'a mut DirtyTracker>,
    pinned: bool,
}

impl<'a> PageGuardMut<'a> {
    pub(crate) fn new(frame: &'a mut BufferFrame) -> AndromedaResult<Self> {
        frame.pin()?;
        Ok(Self {
            frame,
            dirty_tracker: None,
            pinned: true,
        })
    }

    pub(crate) fn new_with_dirty_tracker(
        frame: &'a mut BufferFrame,
        dirty_tracker: &'a mut DirtyTracker,
    ) -> AndromedaResult<Self> {
        frame.pin()?;
        Ok(Self {
            frame,
            dirty_tracker: Some(dirty_tracker),
            pinned: true,
        })
    }

    pub fn frame_id(&self) -> BufferFrameId {
        self.frame.id()
    }

    pub fn page_id(&self) -> Option<PageId> {
        self.frame.page_id()
    }

    pub fn image(&self) -> Option<&PageImage> {
        self.frame.image()
    }

    pub fn layout_contract(&self) -> Option<PageLayoutContract> {
        self.frame.layout_contract()
    }

    pub fn mark_dirty(&mut self, dirty_lsn: Lsn) -> AndromedaResult<()> {
        self.frame.mark_dirty(dirty_lsn)?;
        if let (Some(page_id), Some(dirty_tracker)) =
            (self.frame.page_id(), &mut self.dirty_tracker)
        {
            dirty_tracker.mark_dirty(page_id, dirty_lsn)?;
        }
        Ok(())
    }

    pub fn dirty_image_mut(&mut self, dirty_lsn: Lsn) -> AndromedaResult<Option<&mut PageImage>> {
        self.mark_dirty(dirty_lsn)?;
        Ok(self.frame.image_mut())
    }

    pub fn is_dirty(&self) -> bool {
        self.frame.is_dirty()
    }

    pub fn first_dirty_lsn(&self) -> Option<Lsn> {
        self.frame.first_dirty_lsn()
    }

    pub fn pin_count(&self) -> u32 {
        self.frame.pin_count()
    }

    pub fn unpin(mut self) -> AndromedaResult<()> {
        self.unpin_once()
    }

    fn unpin_once(&mut self) -> AndromedaResult<()> {
        if self.pinned {
            self.frame.unpin()?;
            self.pinned = false;
        }
        Ok(())
    }
}

impl Drop for PageGuardMut<'_> {
    fn drop(&mut self) {
        if self.pinned {
            let _ = self.frame.unpin();
            self.pinned = false;
        }
    }
}
