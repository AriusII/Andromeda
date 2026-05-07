use andromeda_core::AndromedaResult;
use std::sync::Arc;

use super::{ArchiveStatus, WalGcAuditEvent, WalGcCandidate, WalGcContext, WalGcSummary};

/// WAL Garbage Collector: identifies and removes obsolete segments.
pub struct WalGarbageCollector {
    context: Arc<dyn WalGcContext>,
}

impl WalGarbageCollector {
    /// Create a new GC coordinator with the given context.
    pub fn new(context: Arc<dyn WalGcContext>) -> Self {
        WalGarbageCollector { context }
    }

    /// Identify segments eligible for garbage collection.
    ///
    /// Returns candidates sorted by creation_lsn (oldest first) for deterministic ordering.
    /// A segment is a candidate if:
    /// - Its sealing_lsn < min_active_snapshot_lsn (all records are invisible)
    /// - It contains only committed transactions
    /// - Its creation_lsn > required_wal_start_lsn (not needed for recovery)
    pub fn identify_candidates(&self) -> AndromedaResult<Vec<WalGcCandidate>> {
        let min_lsn = self.context.min_active_snapshot_lsn();
        let req_lsn = self.context.required_wal_start_lsn();

        let mut candidates = self.context.identify_gc_candidates(min_lsn)?;

        // Filter by recovery boundary and sort by creation_lsn
        candidates.retain(|c| c.is_eligible(min_lsn, req_lsn));
        candidates.sort_by_key(|c| c.creation_lsn);

        // Emit audit for each candidate
        for candidate in &candidates {
            self.context
                .emit_audit_event(WalGcAuditEvent::CandidateIdentified {
                    segment_id: candidate.segment_id,
                    creation_lsn: candidate.creation_lsn,
                    sealing_lsn: candidate.sealing_lsn,
                    size_bytes: candidate.size_bytes,
                })?;
        }

        Ok(candidates)
    }

    /// Verify and remove a segment after archive confirmation.
    ///
    /// Steps:
    /// 1. Verify segment is archived
    /// 2. If not archived, fail-safe: return error
    /// 3. Delegate to context for safe file removal
    /// 4. Emit audit event on success
    pub fn remove_if_archived(&self, candidate: &WalGcCandidate) -> AndromedaResult<bool> {
        let status = self.context.verify_archived(candidate)?;

        self.context
            .emit_audit_event(WalGcAuditEvent::ArchiveVerifyRequested {
                segment_id: candidate.segment_id,
                archive_status: status,
            })?;

        if status != ArchiveStatus::Archived {
            return Ok(false); // Fail-safe: don't remove if not confirmed archived
        }

        self.context.safe_remove_segment(candidate)?;

        self.context
            .emit_audit_event(WalGcAuditEvent::SegmentRemoved {
                segment_id: candidate.segment_id,
                bytes_freed: candidate.size_bytes,
            })?;

        Ok(true)
    }

    /// Execute a single GC run: identify, verify, and remove candidates.
    ///
    /// Returns a summary with metrics. On any error during identification,
    /// the entire run fails. Archive verification failures are non-fatal
    /// (they just block removal of affected segments).
    pub fn run_gc(&self, run_id: u64) -> AndromedaResult<WalGcSummary> {
        let mut summary = WalGcSummary::new(run_id);

        let candidates = self.identify_candidates()?;
        summary.candidates_identified = candidates.len() as u64;

        let mut blocked = 0u64;
        for candidate in candidates {
            match self.remove_if_archived(&candidate)? {
                true => {
                    summary.segments_removed += 1;
                    summary.bytes_freed += candidate.size_bytes;
                    summary.candidates_archived += 1;
                }
                false => {
                    blocked += 1;
                    summary.candidates_archived += 1; // Counted but not removed
                }
            }
        }
        summary.candidates_blocked = blocked;

        self.context.emit_audit_event(WalGcAuditEvent::Summary {
            run_id,
            candidates_identified: summary.candidates_identified,
            candidates_archived: summary.candidates_archived,
            candidates_blocked: summary.candidates_blocked,
            bytes_freed: summary.bytes_freed,
            segments_removed: summary.segments_removed,
        })?;

        Ok(summary)
    }
}
