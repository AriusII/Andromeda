use andromeda_error::AndromedaResult;
use andromeda_storage_page::PageStore;

use super::BufferPool;
use crate::{
    BufferPoolError, FlushAllDirtyResult, FlushBlockedFrame, FlushError, FlushReadiness,
    FlushStorageOperation, WalDurabilityObserver, classify_flush_candidate,
};

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
            let FlushReadiness::Ready = classify_flush_candidate(
                page_id.get(),
                candidate.first_dirty_lsn(),
                last_dirty_lsn,
                observer,
            ) else {
                continue;
            };
            let durable_lsn = observer.max_durable_lsn();

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
            let readiness =
                classify_flush_candidate(page_id.get(), first_dirty_lsn, last_dirty_lsn, observer);
            let FlushReadiness::Ready = readiness else {
                let FlushReadiness::Blocked(blocked) = readiness else {
                    continue;
                };
                result
                    .blocked_by_wal_durability
                    .push(FlushBlockedFrame::from_core(blocked));
                continue;
            };
            let durable_lsn = observer.max_durable_lsn();

            let index = match self.page_table.get(&page_id) {
                Some(&idx) => idx,
                None => {
                    result.errors.push(FlushError::FrameNotResident { page_id });
                    continue;
                },
            };

            let pin_count = self.frames[index].pin_count();
            if pin_count != 0 {
                result
                    .errors
                    .push(FlushError::FramePinned { page_id, pin_count });
                continue;
            }

            let image = match self.frames[index].image().cloned() {
                Some(img) => img,
                None => {
                    result.errors.push(FlushError::InvalidPageImage { page_id });
                    continue;
                },
            };

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
