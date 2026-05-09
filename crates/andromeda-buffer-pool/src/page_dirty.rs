use andromeda_error::AndromedaResult;
use andromeda_storage_page::PageId;

use andromeda_wal::Lsn;

use crate::dirty as core_dirty;
use crate::pool_error::map_core_error;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DirtyTracker {
    core: core_dirty::DirtyTracker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyEntry {
    core: core_dirty::DirtyEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyFlushCandidate {
    core: core_dirty::DirtyFlushCandidate,
}

impl DirtyTracker {
    pub const fn new() -> Self {
        Self {
            core: core_dirty::DirtyTracker::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.core.len()
    }

    pub fn is_empty(&self) -> bool {
        self.core.is_empty()
    }

    pub fn mark_dirty(&mut self, page_id: PageId, dirty_lsn: Lsn) -> AndromedaResult<()> {
        self.core
            .mark_dirty(page_id.get(), dirty_lsn)
            .map_err(map_core_error)
    }

    pub fn mark_clean(&mut self, page_id: PageId) -> AndromedaResult<bool> {
        self.core.mark_clean(page_id.get()).map_err(map_core_error)
    }

    pub fn mark_clean_after_flush(&mut self, page_id: PageId) -> AndromedaResult<bool> {
        self.core
            .mark_clean_after_flush(page_id.get())
            .map_err(map_core_error)
    }

    pub fn contains(&self, page_id: PageId) -> AndromedaResult<bool> {
        self.core.contains(page_id.get()).map_err(map_core_error)
    }

    pub fn is_dirty(&self, page_id: PageId) -> AndromedaResult<bool> {
        self.core.is_dirty(page_id.get()).map_err(map_core_error)
    }

    pub fn first_dirty_lsn(&self, page_id: PageId) -> AndromedaResult<Option<Lsn>> {
        self.core
            .first_dirty_lsn(page_id.get())
            .map_err(map_core_error)
    }

    pub fn last_dirty_lsn(&self, page_id: PageId) -> AndromedaResult<Option<Lsn>> {
        self.core
            .last_dirty_lsn(page_id.get())
            .map_err(map_core_error)
    }

    pub fn dirty_entry(&self, page_id: PageId) -> AndromedaResult<Option<DirtyEntry>> {
        self.core
            .dirty_entry(page_id.get())
            .map(|entry| entry.map(DirtyEntry::from_core))
            .map_err(map_core_error)
    }

    pub fn flush_candidates(&self) -> Vec<DirtyFlushCandidate> {
        self.core
            .flush_candidates()
            .into_iter()
            .map(DirtyFlushCandidate::from_core)
            .collect()
    }

    pub fn ordered_flush_candidates(&self) -> Vec<DirtyFlushCandidate> {
        self.flush_candidates()
    }
}

impl DirtyEntry {
    const fn from_core(core: core_dirty::DirtyEntry) -> Self {
        Self { core }
    }

    pub const fn page_id(self) -> PageId {
        PageId::new(self.core.page_id())
    }

    pub const fn first_dirty_lsn(self) -> Lsn {
        self.core.first_dirty_lsn()
    }

    pub const fn last_dirty_lsn(self) -> Lsn {
        self.core.last_dirty_lsn()
    }
}

impl DirtyFlushCandidate {
    pub(crate) const fn from_core(core: core_dirty::DirtyFlushCandidate) -> Self {
        Self { core }
    }

    pub const fn page_id(self) -> PageId {
        PageId::new(self.core.page_id())
    }

    pub const fn first_dirty_lsn(self) -> Lsn {
        self.core.first_dirty_lsn()
    }

    pub const fn last_dirty_lsn(self) -> Lsn {
        self.core.last_dirty_lsn()
    }
}

#[cfg(test)]
mod tests {
    use andromeda_error::AndromedaErrorKind;

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
        assert_eq!(
            tracker
                .last_dirty_lsn(PageId::new(10))
                .expect("valid lsn query"),
            Some(Lsn::new(100))
        );
    }

    #[test]
    fn duplicate_dirty_mark_preserves_dirty_lsn_range_without_duplicates() {
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
        assert_eq!(
            tracker
                .last_dirty_lsn(PageId::new(11))
                .expect("valid lsn query"),
            Some(Lsn::new(120))
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
                .map(|candidate| {
                    (
                        candidate.page_id(),
                        candidate.first_dirty_lsn(),
                        candidate.last_dirty_lsn(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                (PageId::new(10), Lsn::new(200), Lsn::new(200)),
                (PageId::new(20), Lsn::new(200), Lsn::new(200)),
                (PageId::new(30), Lsn::new(300), Lsn::new(300)),
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
