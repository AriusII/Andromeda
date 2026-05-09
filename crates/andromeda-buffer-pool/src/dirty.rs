use std::collections::BTreeMap;

use andromeda_wal::Lsn;

use crate::error::BufferPoolCoreError;
use crate::frame::validate_page_id_value;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DirtyTracker {
    dirty_pages: BTreeMap<u64, DirtyEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyEntry {
    page_id: u64,
    first_dirty_lsn: Lsn,
    last_dirty_lsn: Lsn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyFlushCandidate {
    page_id: u64,
    first_dirty_lsn: Lsn,
    last_dirty_lsn: Lsn,
}

impl DirtyTracker {
    pub const fn new() -> Self {
        Self {
            dirty_pages: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.dirty_pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dirty_pages.is_empty()
    }

    pub fn mark_dirty(&mut self, page_id: u64, dirty_lsn: Lsn) -> Result<(), BufferPoolCoreError> {
        validate_page_id_value(page_id)?;
        validate_dirty_lsn(dirty_lsn)?;

        self.dirty_pages
            .entry(page_id)
            .and_modify(|entry| {
                if dirty_lsn < entry.first_dirty_lsn {
                    entry.first_dirty_lsn = dirty_lsn;
                }
                if dirty_lsn > entry.last_dirty_lsn {
                    entry.last_dirty_lsn = dirty_lsn;
                }
            })
            .or_insert(DirtyEntry {
                page_id,
                first_dirty_lsn: dirty_lsn,
                last_dirty_lsn: dirty_lsn,
            });

        Ok(())
    }

    pub fn mark_clean(&mut self, page_id: u64) -> Result<bool, BufferPoolCoreError> {
        validate_page_id_value(page_id)?;
        Ok(self.dirty_pages.remove(&page_id).is_some())
    }

    pub fn mark_clean_after_flush(&mut self, page_id: u64) -> Result<bool, BufferPoolCoreError> {
        self.mark_clean(page_id)
    }

    pub fn contains(&self, page_id: u64) -> Result<bool, BufferPoolCoreError> {
        validate_page_id_value(page_id)?;
        Ok(self.dirty_pages.contains_key(&page_id))
    }

    pub fn is_dirty(&self, page_id: u64) -> Result<bool, BufferPoolCoreError> {
        self.contains(page_id)
    }

    pub fn first_dirty_lsn(&self, page_id: u64) -> Result<Option<Lsn>, BufferPoolCoreError> {
        validate_page_id_value(page_id)?;
        Ok(self
            .dirty_pages
            .get(&page_id)
            .copied()
            .map(DirtyEntry::first_dirty_lsn))
    }

    pub fn last_dirty_lsn(&self, page_id: u64) -> Result<Option<Lsn>, BufferPoolCoreError> {
        validate_page_id_value(page_id)?;
        Ok(self
            .dirty_pages
            .get(&page_id)
            .copied()
            .map(DirtyEntry::last_dirty_lsn))
    }

    pub fn dirty_entry(&self, page_id: u64) -> Result<Option<DirtyEntry>, BufferPoolCoreError> {
        validate_page_id_value(page_id)?;
        Ok(self.dirty_pages.get(&page_id).copied())
    }

    pub fn flush_candidates(&self) -> Vec<DirtyFlushCandidate> {
        let mut candidates: Vec<_> = self
            .dirty_pages
            .values()
            .map(|entry| DirtyFlushCandidate {
                page_id: entry.page_id,
                first_dirty_lsn: entry.first_dirty_lsn,
                last_dirty_lsn: entry.last_dirty_lsn,
            })
            .collect();
        candidates.sort_by_key(|candidate| (candidate.first_dirty_lsn, candidate.page_id));
        candidates
    }

    pub fn ordered_flush_candidates(&self) -> Vec<DirtyFlushCandidate> {
        self.flush_candidates()
    }
}

impl DirtyEntry {
    pub const fn page_id(self) -> u64 {
        self.page_id
    }

    pub const fn first_dirty_lsn(self) -> Lsn {
        self.first_dirty_lsn
    }

    pub const fn last_dirty_lsn(self) -> Lsn {
        self.last_dirty_lsn
    }
}

impl DirtyFlushCandidate {
    pub const fn page_id(self) -> u64 {
        self.page_id
    }

    pub const fn first_dirty_lsn(self) -> Lsn {
        self.first_dirty_lsn
    }

    pub const fn last_dirty_lsn(self) -> Lsn {
        self.last_dirty_lsn
    }
}

fn validate_dirty_lsn(dirty_lsn: Lsn) -> Result<(), BufferPoolCoreError> {
    if dirty_lsn.is_zero() {
        return Err(BufferPoolCoreError::InvalidDirtyLsn);
    }
    Ok(())
}
