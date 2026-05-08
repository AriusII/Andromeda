//! Flush result types for flush_all_dirty contract.
//!
//! This module defines the result type returned by [`flush_all_dirty_with_report`].
//! It provides visibility into which frames were flushed, which were blocked awaiting
//! WAL durability, and which encountered errors during the flush attempt.

use crate::{Lsn, PageId};

/// Complete flush result with blocked state reporting.
///
/// Returned by `BufferPool::flush_all_dirty_with_report`, this result provides
/// the caller with explicit visibility into the outcome of a full dirty flush cycle:
///
/// - `flushed`: Count of pages successfully written to durable storage
/// - `blocked_by_wal_durability`: Pages that could not be flushed because their
///   latest dirty LSN was not yet durable in the WAL
/// - `errors`: Pages that encountered flush errors (pinned, IO errors, etc.)
///
/// Invariant: A page appears in exactly one of these lists: flushed, blocked, or errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlushAllDirtyResult {
    /// Count of successfully flushed pages.
    pub flushed: usize,

    /// Pages blocked awaiting WAL durability. These pages remain dirty and should
    /// be retried after the WAL advances further.
    pub blocked_by_wal_durability: Vec<FlushBlockedFrame>,

    /// Pages that encountered errors during flush. These pages remain dirty.
    pub errors: Vec<FlushError>,
}

/// Metadata about a frame blocked on WAL durability during a flush attempt.
///
/// This provides the caller with actionable information about why a frame could not
/// be flushed and what LSN it is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlushBlockedFrame {
    /// The page ID of the blocked frame.
    pub page_id: PageId,

    /// The first LSN at which this page became dirty. This LSN must be durable
    /// in the WAL before the frame can be flushed, but it is not sufficient
    /// when the page has later dirty updates.
    pub first_dirty_lsn: Lsn,

    /// The latest dirty LSN observed for this page. This is the durability
    /// fence LSN that must be covered by durable WAL before flush.
    pub last_dirty_lsn: Lsn,

    /// The maximum LSN currently durable in the WAL. The frame is blocked because
    /// `last_dirty_lsn > max_durable_lsn`.
    pub max_durable_lsn: Lsn,
}

impl FlushBlockedFrame {
    /// Create a blocked frame record.
    pub fn new(page_id: PageId, first_dirty_lsn: Lsn, max_durable_lsn: Lsn) -> Self {
        Self::new_with_last_dirty_lsn(page_id, first_dirty_lsn, first_dirty_lsn, max_durable_lsn)
    }

    /// Create a blocked frame record carrying the full dirty LSN range.
    pub fn new_with_last_dirty_lsn(
        page_id: PageId,
        first_dirty_lsn: Lsn,
        last_dirty_lsn: Lsn,
        max_durable_lsn: Lsn,
    ) -> Self {
        Self {
            page_id,
            first_dirty_lsn,
            last_dirty_lsn,
            max_durable_lsn,
        }
    }

    /// LSN gap that must close before this frame can flush.
    pub fn lsn_gap(&self) -> u64 {
        self.last_dirty_lsn
            .get()
            .saturating_sub(self.max_durable_lsn.get())
    }

    pub(crate) fn from_core(core: andromeda_buffer_pool::FlushBlockedFrameCore) -> Self {
        Self {
            page_id: PageId::new(core.page_id()),
            first_dirty_lsn: core.first_dirty_lsn(),
            last_dirty_lsn: core.last_dirty_lsn(),
            max_durable_lsn: core.max_durable_lsn(),
        }
    }
}

/// Storage operation that failed during dirty-page flush.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlushStorageOperation {
    PageStoreWrite,
    DirtyTrackerMarkClean,
}

/// Error encountered while attempting to flush a specific page.
///
/// These are recovery-safe: frames with flush errors remain dirty and can be
/// retried, flushed to different locations, or handled by recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlushError {
    /// The page is not resident in the buffer pool.
    FrameNotResident { page_id: PageId },

    /// The frame is pinned and cannot be flushed.
    FramePinned { page_id: PageId, pin_count: u32 },

    /// A typed storage-layer error occurred during flush.
    StorageError {
        page_id: PageId,
        operation: FlushStorageOperation,
        message: String,
    },

    /// The frame is in an invalid state for flush.
    InvalidFrameState { page_id: PageId },

    /// The page image was invalid or corrupted.
    InvalidPageImage { page_id: PageId },
}

impl FlushError {
    /// Get the page ID associated with this error.
    pub fn page_id(&self) -> Option<PageId> {
        match self {
            Self::FrameNotResident { page_id } => Some(*page_id),
            Self::FramePinned { page_id, .. } => Some(*page_id),
            Self::StorageError { page_id, .. } => Some(*page_id),
            Self::InvalidFrameState { page_id } => Some(*page_id),
            Self::InvalidPageImage { page_id } => Some(*page_id),
        }
    }

    /// Convert a storage error into an observable page-scoped flush error.
    pub fn storage(
        page_id: PageId,
        operation: FlushStorageOperation,
        message: impl Into<String>,
    ) -> Self {
        Self::StorageError {
            page_id,
            operation,
            message: message.into(),
        }
    }
}

impl FlushAllDirtyResult {
    /// Create a new empty flush result.
    pub fn new() -> Self {
        Self {
            flushed: 0,
            blocked_by_wal_durability: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Total number of pages examined during flush (flushed + blocked + errors).
    pub fn total_examined(&self) -> usize {
        self.flushed + self.blocked_by_wal_durability.len() + self.errors.len()
    }

    /// Check if all flush attempts succeeded.
    pub fn all_succeeded(&self) -> bool {
        self.blocked_by_wal_durability.is_empty() && self.errors.is_empty()
    }

    /// Check if any flush attempt encountered an error.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check if any frames are blocked on WAL durability.
    pub fn has_blocked(&self) -> bool {
        !self.blocked_by_wal_durability.is_empty()
    }
}

impl Default for FlushAllDirtyResult {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_frame_computes_lsn_gap() {
        let blocked = FlushBlockedFrame::new(PageId::new(42), Lsn::new(100), Lsn::new(50));
        assert_eq!(blocked.lsn_gap(), 50);
    }

    #[test]
    fn blocked_frame_lsn_gap_saturates() {
        let blocked = FlushBlockedFrame::new(PageId::new(42), Lsn::new(50), Lsn::new(100));
        assert_eq!(blocked.lsn_gap(), 0);
    }

    #[test]
    fn flush_error_extracts_page_id() {
        assert_eq!(
            FlushError::FrameNotResident {
                page_id: PageId::new(10)
            }
            .page_id(),
            Some(PageId::new(10))
        );
        assert_eq!(
            FlushError::FramePinned {
                page_id: PageId::new(11),
                pin_count: 2
            }
            .page_id(),
            Some(PageId::new(11))
        );
        assert_eq!(
            FlushError::storage(
                PageId::new(12),
                FlushStorageOperation::PageStoreWrite,
                "io error",
            )
            .page_id(),
            Some(PageId::new(12))
        );
    }

    #[test]
    fn flush_result_counts_examined() {
        let mut result = FlushAllDirtyResult::new();
        result.flushed = 3;
        result.blocked_by_wal_durability = vec![
            FlushBlockedFrame::new(PageId::new(1), Lsn::new(10), Lsn::new(5)),
            FlushBlockedFrame::new(PageId::new(2), Lsn::new(20), Lsn::new(5)),
        ];
        result.errors = vec![FlushError::FramePinned {
            page_id: PageId::new(3),
            pin_count: 1,
        }];

        assert_eq!(result.total_examined(), 6);
        assert!(!result.all_succeeded());
        assert!(result.has_errors());
        assert!(result.has_blocked());
    }

    #[test]
    fn flush_result_all_succeeded_when_no_blocked_or_errors() {
        let mut result = FlushAllDirtyResult::new();
        result.flushed = 5;

        assert!(result.all_succeeded());
        assert!(!result.has_errors());
        assert!(!result.has_blocked());
        assert_eq!(result.total_examined(), 5);
    }
}
