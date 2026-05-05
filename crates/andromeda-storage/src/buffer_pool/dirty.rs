use std::collections::BTreeMap;

use andromeda_core::AndromedaResult;

use crate::{Lsn, PageId};

use super::BufferPoolError;

/// Deterministic dirty-page tracker for transient buffer-pool metadata.
///
/// The tracker indexes dirty residency state by durable [`PageId`] but does not
/// own page images, page layout, WAL contents, or flush IO. It records only the
/// first dirty LSN needed by future buffer-pool flush scheduling.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DirtyTracker {
    dirty_pages: BTreeMap<PageId, DirtyEntry>,
}

/// Dirty metadata associated with one resident page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyEntry {
    page_id: PageId,
    first_dirty_lsn: Lsn,
}

/// Stable flush candidate ordered by first dirty LSN and then page id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyFlushCandidate {
    page_id: PageId,
    first_dirty_lsn: Lsn,
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

    /// Mark a page dirty while preserving the earliest first-dirty LSN observed
    /// for that page.
    ///
    /// Repeated marks for the same page update the existing entry in place, so
    /// the tracker never creates duplicate dirty records for a page.
    pub fn mark_dirty(&mut self, page_id: PageId, dirty_lsn: Lsn) -> AndromedaResult<()> {
        validate_page_id(page_id)?;
        validate_dirty_lsn(dirty_lsn)?;

        self.dirty_pages
            .entry(page_id)
            .and_modify(|entry| {
                if dirty_lsn < entry.first_dirty_lsn {
                    entry.first_dirty_lsn = dirty_lsn;
                }
            })
            .or_insert(DirtyEntry {
                page_id,
                first_dirty_lsn: dirty_lsn,
            });

        Ok(())
    }

    /// Remove a page from the dirty set after a successful flush.
    ///
    /// Returns `true` when an entry existed and was removed.
    pub fn mark_clean(&mut self, page_id: PageId) -> AndromedaResult<bool> {
        validate_page_id(page_id)?;
        Ok(self.dirty_pages.remove(&page_id).is_some())
    }

    pub fn mark_clean_after_flush(&mut self, page_id: PageId) -> AndromedaResult<bool> {
        self.mark_clean(page_id)
    }

    pub fn contains(&self, page_id: PageId) -> AndromedaResult<bool> {
        validate_page_id(page_id)?;
        Ok(self.dirty_pages.contains_key(&page_id))
    }

    pub fn is_dirty(&self, page_id: PageId) -> AndromedaResult<bool> {
        self.contains(page_id)
    }

    pub fn first_dirty_lsn(&self, page_id: PageId) -> AndromedaResult<Option<Lsn>> {
        validate_page_id(page_id)?;
        Ok(self
            .dirty_pages
            .get(&page_id)
            .copied()
            .map(DirtyEntry::first_dirty_lsn))
    }

    pub fn dirty_entry(&self, page_id: PageId) -> AndromedaResult<Option<DirtyEntry>> {
        validate_page_id(page_id)?;
        Ok(self.dirty_pages.get(&page_id).copied())
    }

    /// Produce deterministic flush candidates ordered by oldest first dirty LSN,
    /// with [`PageId`] as the stable tie-breaker.
    pub fn flush_candidates(&self) -> Vec<DirtyFlushCandidate> {
        let mut candidates: Vec<_> = self
            .dirty_pages
            .values()
            .map(|entry| DirtyFlushCandidate {
                page_id: entry.page_id,
                first_dirty_lsn: entry.first_dirty_lsn,
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
    pub const fn page_id(self) -> PageId {
        self.page_id
    }

    pub const fn first_dirty_lsn(self) -> Lsn {
        self.first_dirty_lsn
    }
}

impl DirtyFlushCandidate {
    pub const fn page_id(self) -> PageId {
        self.page_id
    }

    pub const fn first_dirty_lsn(self) -> Lsn {
        self.first_dirty_lsn
    }
}

fn validate_page_id(page_id: PageId) -> AndromedaResult<()> {
    if page_id.is_zero() {
        return Err(BufferPoolError::InvalidPageId.into_andromeda_error());
    }
    Ok(())
}

fn validate_dirty_lsn(dirty_lsn: Lsn) -> AndromedaResult<()> {
    if dirty_lsn.is_zero() {
        return Err(BufferPoolError::InvalidDirtyLsn.into_andromeda_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use andromeda_core::AndromedaErrorKind;

    use super::*;

    #[test]
    fn mark_dirty_inserts_entry() {
        let mut tracker = DirtyTracker::new();

        tracker
            .mark_dirty(PageId::new(10), Lsn::new(100))
            .expect("valid dirty mark");

        assert_eq!(tracker.len(), 1);
        assert!(
            tracker
                .contains(PageId::new(10))
                .expect("valid membership query")
        );
        assert_eq!(
            tracker
                .first_dirty_lsn(PageId::new(10))
                .expect("valid lsn query"),
            Some(Lsn::new(100))
        );
    }

    #[test]
    fn duplicate_dirty_mark_preserves_earliest_lsn_without_duplicates() {
        let mut tracker = DirtyTracker::new();

        tracker
            .mark_dirty(PageId::new(11), Lsn::new(100))
            .expect("first dirty mark");
        tracker
            .mark_dirty(PageId::new(11), Lsn::new(120))
            .expect("later duplicate dirty mark");
        tracker
            .mark_dirty(PageId::new(11), Lsn::new(90))
            .expect("earlier dirty observation");

        assert_eq!(tracker.len(), 1);
        assert_eq!(
            tracker
                .first_dirty_lsn(PageId::new(11))
                .expect("valid lsn query"),
            Some(Lsn::new(90))
        );
        assert_eq!(tracker.flush_candidates().len(), 1);
    }

    #[test]
    fn ordered_flush_candidates_use_oldest_lsn_then_page_id() {
        let mut tracker = DirtyTracker::new();
        tracker
            .mark_dirty(PageId::new(30), Lsn::new(300))
            .expect("dirty mark");
        tracker
            .mark_dirty(PageId::new(20), Lsn::new(200))
            .expect("dirty mark");
        tracker
            .mark_dirty(PageId::new(10), Lsn::new(200))
            .expect("dirty mark");

        let candidates = tracker.flush_candidates();

        assert_eq!(
            candidates
                .iter()
                .map(|candidate| (candidate.page_id(), candidate.first_dirty_lsn()))
                .collect::<Vec<_>>(),
            vec![
                (PageId::new(10), Lsn::new(200)),
                (PageId::new(20), Lsn::new(200)),
                (PageId::new(30), Lsn::new(300)),
            ]
        );
    }

    #[test]
    fn clean_removal_removes_membership() {
        let mut tracker = DirtyTracker::new();
        tracker
            .mark_dirty(PageId::new(12), Lsn::new(110))
            .expect("dirty mark");

        assert!(tracker.mark_clean(PageId::new(12)).expect("clean removal"));
        assert!(
            !tracker
                .contains(PageId::new(12))
                .expect("valid membership query")
        );
        assert!(
            !tracker
                .mark_clean(PageId::new(12))
                .expect("idempotent clean removal")
        );
        assert!(tracker.is_empty());
    }

    #[test]
    fn invalid_zero_page_or_lsn_return_storage_errors() {
        let mut tracker = DirtyTracker::new();

        assert_eq!(
            tracker
                .mark_dirty(PageId::new(0), Lsn::new(1))
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
        assert_eq!(
            tracker
                .mark_dirty(PageId::new(1), Lsn::ZERO)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
        assert_eq!(
            tracker.contains(PageId::new(0)).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
        assert_eq!(
            tracker.mark_clean(PageId::new(0)).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
