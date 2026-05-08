use andromeda_wal::Lsn;

use crate::error::BufferPoolCoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferFrameId(usize);

impl BufferFrameId {
    pub fn new(value: usize) -> Result<Self, BufferPoolCoreError> {
        let id = Self(value);
        id.validate()?;
        Ok(id)
    }

    pub fn validate(self) -> Result<(), BufferPoolCoreError> {
        if self.0 == 0 {
            return Err(BufferPoolCoreError::InvalidFrameId { frame_id: self.0 });
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferFrameState {
    Free,
    Resident,
    Flushing,
    Evicting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferFrameCore {
    id: BufferFrameId,
    state: BufferFrameState,
    page_id: Option<u64>,
    page_lsn: Option<Lsn>,
    pin_count: u32,
    is_dirty: bool,
    first_dirty_lsn: Option<Lsn>,
    last_dirty_lsn: Option<Lsn>,
    clock_usage: bool,
}

impl BufferFrameCore {
    pub fn free(id: BufferFrameId) -> Result<Self, BufferPoolCoreError> {
        let frame = Self {
            id,
            state: BufferFrameState::Free,
            page_id: None,
            page_lsn: None,
            pin_count: 0,
            is_dirty: false,
            first_dirty_lsn: None,
            last_dirty_lsn: None,
            clock_usage: false,
        };
        frame.validate()?;
        Ok(frame)
    }

    pub fn resident(
        id: BufferFrameId,
        page_id: u64,
        page_lsn: Lsn,
    ) -> Result<Self, BufferPoolCoreError> {
        validate_page_id_value(page_id)?;
        validate_lsn(page_lsn)?;
        let frame = Self {
            id,
            state: BufferFrameState::Resident,
            page_id: Some(page_id),
            page_lsn: Some(page_lsn),
            pin_count: 0,
            is_dirty: false,
            first_dirty_lsn: None,
            last_dirty_lsn: None,
            clock_usage: true,
        };
        frame.validate()?;
        Ok(frame)
    }

    pub fn validate(&self) -> Result<(), BufferPoolCoreError> {
        self.id.validate()?;
        match (self.is_dirty, self.first_dirty_lsn, self.last_dirty_lsn) {
            (true, Some(first), Some(last))
                if !first.is_zero() && !last.is_zero() && first <= last => {}
            (false, None, None) => {}
            _ => return Err(BufferPoolCoreError::InvalidDirtyLsn),
        }

        match self.state {
            BufferFrameState::Free => {
                if self.page_id.is_some()
                    || self.page_lsn.is_some()
                    || self.pin_count != 0
                    || self.is_dirty
                    || self.first_dirty_lsn.is_some()
                    || self.last_dirty_lsn.is_some()
                {
                    return Err(BufferPoolCoreError::InvalidFrameState);
                }
                if self.clock_usage {
                    return Err(BufferPoolCoreError::InvalidClockUsage);
                }
            }
            BufferFrameState::Resident
            | BufferFrameState::Flushing
            | BufferFrameState::Evicting => {
                self.validate_resident_metadata()?;
                if matches!(self.state, BufferFrameState::Flushing) && self.pin_count != 0 {
                    return Err(BufferPoolCoreError::InvalidFrameState);
                }
                if matches!(self.state, BufferFrameState::Evicting)
                    && (self.pin_count != 0 || self.is_dirty)
                {
                    return Err(BufferPoolCoreError::InvalidFrameState);
                }
            }
        }
        Ok(())
    }

    fn validate_resident_metadata(&self) -> Result<(), BufferPoolCoreError> {
        let page_id = self.page_id.ok_or(BufferPoolCoreError::InvalidPageId)?;
        validate_page_id_value(page_id)?;
        let page_lsn = self.page_lsn.ok_or(BufferPoolCoreError::InvalidPageImage)?;
        validate_lsn(page_lsn)?;

        if let Some(first_dirty_lsn) = self.first_dirty_lsn
            && (first_dirty_lsn.is_zero() || first_dirty_lsn < page_lsn)
        {
            return Err(BufferPoolCoreError::InvalidDirtyLsn);
        }
        if let Some(last_dirty_lsn) = self.last_dirty_lsn
            && (last_dirty_lsn.is_zero() || last_dirty_lsn < page_lsn)
        {
            return Err(BufferPoolCoreError::InvalidDirtyLsn);
        }
        Ok(())
    }

    pub const fn id(&self) -> BufferFrameId {
        self.id
    }

    pub const fn state(&self) -> BufferFrameState {
        self.state
    }

    pub const fn page_id_value(&self) -> Option<u64> {
        self.page_id
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

    pub const fn last_dirty_lsn(&self) -> Option<Lsn> {
        self.last_dirty_lsn
    }

    pub const fn clock_usage(&self) -> bool {
        self.clock_usage
    }

    pub fn pin(&mut self) -> Result<(), BufferPoolCoreError> {
        if self.state != BufferFrameState::Resident {
            return Err(BufferPoolCoreError::InvalidFrameState);
        }
        self.pin_count = self
            .pin_count
            .checked_add(1)
            .ok_or(BufferPoolCoreError::InvalidPinCount)?;
        self.set_clock_usage()?;
        Ok(())
    }

    pub fn unpin(&mut self) -> Result<(), BufferPoolCoreError> {
        if self.state != BufferFrameState::Resident {
            return Err(BufferPoolCoreError::InvalidFrameState);
        }
        self.pin_count = self
            .pin_count
            .checked_sub(1)
            .ok_or(BufferPoolCoreError::InvalidPinCount)?;
        Ok(())
    }

    pub fn mark_dirty(&mut self, dirty_lsn: Lsn) -> Result<(), BufferPoolCoreError> {
        if self.state != BufferFrameState::Resident {
            return Err(BufferPoolCoreError::InvalidFrameState);
        }
        if self.pin_count == 0 {
            return Err(BufferPoolCoreError::InvalidPinCount);
        }
        let page_lsn = self.page_lsn.ok_or(BufferPoolCoreError::InvalidPageImage)?;
        if dirty_lsn.is_zero() || dirty_lsn < page_lsn {
            return Err(BufferPoolCoreError::InvalidDirtyLsn);
        }
        if let Some(last_dirty_lsn) = self.last_dirty_lsn {
            if dirty_lsn < last_dirty_lsn {
                return Err(BufferPoolCoreError::InvalidDirtyLsn);
            }
        } else {
            self.first_dirty_lsn = Some(dirty_lsn);
        }
        self.last_dirty_lsn = Some(dirty_lsn);
        self.is_dirty = true;
        self.set_clock_usage()?;
        Ok(())
    }

    pub fn mark_clean_after_flush(&mut self, flushed_lsn: Lsn) -> Result<(), BufferPoolCoreError> {
        if !matches!(
            self.state,
            BufferFrameState::Resident | BufferFrameState::Flushing
        ) {
            return Err(BufferPoolCoreError::InvalidFrameState);
        }
        let first_dirty_lsn = self
            .first_dirty_lsn
            .ok_or(BufferPoolCoreError::InvalidDirtyLsn)?;
        let last_dirty_lsn = self
            .last_dirty_lsn
            .ok_or(BufferPoolCoreError::InvalidDirtyLsn)?;
        if first_dirty_lsn > last_dirty_lsn || flushed_lsn < last_dirty_lsn {
            return Err(BufferPoolCoreError::InvalidDirtyLsn);
        }
        self.is_dirty = false;
        self.first_dirty_lsn = None;
        self.last_dirty_lsn = None;
        self.state = BufferFrameState::Resident;
        self.validate()
    }

    pub fn mark_clean(&mut self) {
        self.is_dirty = false;
        self.first_dirty_lsn = None;
        self.last_dirty_lsn = None;
        if self.state == BufferFrameState::Flushing {
            self.state = BufferFrameState::Resident;
        }
    }

    pub fn set_clock_usage(&mut self) -> Result<(), BufferPoolCoreError> {
        if self.state == BufferFrameState::Free {
            return Err(BufferPoolCoreError::InvalidClockUsage);
        }
        self.clock_usage = true;
        Ok(())
    }

    pub fn consume_clock_usage(&mut self) -> Result<bool, BufferPoolCoreError> {
        if self.state == BufferFrameState::Free {
            return Err(BufferPoolCoreError::InvalidClockUsage);
        }
        let previous = self.clock_usage;
        self.clock_usage = false;
        Ok(previous)
    }

    pub fn begin_flush(&mut self) -> Result<(), BufferPoolCoreError> {
        if self.state != BufferFrameState::Resident || !self.is_dirty || self.pin_count != 0 {
            return Err(BufferPoolCoreError::InvalidFrameState);
        }
        self.state = BufferFrameState::Flushing;
        self.validate()
    }

    pub fn abort_flush(&mut self) -> Result<(), BufferPoolCoreError> {
        if self.state != BufferFrameState::Flushing || !self.is_dirty || self.pin_count != 0 {
            return Err(BufferPoolCoreError::InvalidFrameState);
        }
        self.state = BufferFrameState::Resident;
        self.validate()
    }

    pub fn finish_flush(&mut self, flushed_lsn: Lsn) -> Result<(), BufferPoolCoreError> {
        if self.state != BufferFrameState::Flushing {
            return Err(BufferPoolCoreError::InvalidFrameState);
        }
        self.mark_clean_after_flush(flushed_lsn)
    }

    pub fn begin_eviction(&mut self) -> Result<(), BufferPoolCoreError> {
        if self.state != BufferFrameState::Resident || self.pin_count != 0 || self.is_dirty {
            return Err(BufferPoolCoreError::InvalidFrameState);
        }
        self.state = BufferFrameState::Evicting;
        self.validate()
    }

    pub fn finish_eviction(&mut self) -> Result<(), BufferPoolCoreError> {
        if self.state != BufferFrameState::Evicting {
            return Err(BufferPoolCoreError::InvalidFrameState);
        }
        self.page_id = None;
        self.page_lsn = None;
        self.pin_count = 0;
        self.is_dirty = false;
        self.first_dirty_lsn = None;
        self.last_dirty_lsn = None;
        self.clock_usage = false;
        self.state = BufferFrameState::Free;
        self.validate()
    }
}

pub(crate) fn validate_page_id_value(page_id: u64) -> Result<(), BufferPoolCoreError> {
    if page_id == 0 {
        return Err(BufferPoolCoreError::InvalidPageId);
    }
    Ok(())
}

fn validate_lsn(lsn: Lsn) -> Result<(), BufferPoolCoreError> {
    if lsn.is_zero() {
        return Err(BufferPoolCoreError::InvalidDirtyLsn);
    }
    Ok(())
}
