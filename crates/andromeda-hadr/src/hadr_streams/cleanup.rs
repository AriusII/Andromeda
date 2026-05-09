use std::collections::HashSet;

use super::HadrLogicalStreamId;

/// Orphan stream cleanup handler.
///
/// Detects and cleans up streams that become orphaned due to connection loss,
/// timeout, or explicit disconnection. Cleanup is deterministic and idempotent.
#[derive(Debug)]
pub struct HadrStreamCleanup {
    /// Logical stream IDs to be cleaned up.
    orphan_streams: HashSet<HadrLogicalStreamId>,

    /// Logical stream IDs successfully cleaned (freed).
    cleaned_streams: HashSet<HadrLogicalStreamId>,
}

impl HadrStreamCleanup {
    /// Creates a new cleanup handler.
    pub fn new() -> Self {
        Self {
            orphan_streams: HashSet::new(),
            cleaned_streams: HashSet::new(),
        }
    }

    /// Marks a stream as orphaned.
    ///
    /// The stream will be cleaned up (freed) when `execute_cleanup()` is called.
    pub fn mark_orphan(&mut self, stream_id: u64) {
        self.mark_logical_orphan(HadrLogicalStreamId::new(stream_id));
    }

    /// Marks a logical stream as orphaned.
    pub fn mark_logical_orphan(&mut self, stream_id: HadrLogicalStreamId) {
        self.orphan_streams.insert(stream_id);
    }

    /// Marks multiple streams as orphaned (e.g., all streams for a replica).
    pub fn mark_orphans(&mut self, stream_ids: &[u64]) {
        for &stream_id in stream_ids {
            self.mark_orphan(stream_id);
        }
    }

    /// Executes cleanup for all orphaned streams.
    ///
    /// This operation is idempotent: calling it multiple times is safe.
    /// Returns the set of successfully cleaned stream IDs.
    pub fn execute_cleanup(&mut self) -> HashSet<u64> {
        let cleaned = self.orphan_streams.drain().collect::<HashSet<_>>();
        self.cleaned_streams.extend(cleaned.iter().copied());
        cleaned.into_iter().map(HadrLogicalStreamId::get).collect()
    }

    /// Returns the count of orphaned (pending cleanup) streams.
    pub fn orphan_count(&self) -> usize {
        self.orphan_streams.len()
    }

    /// Returns the count of successfully cleaned streams.
    pub fn cleaned_count(&self) -> usize {
        self.cleaned_streams.len()
    }

    /// Returns true if there are pending orphan streams.
    pub fn has_orphans(&self) -> bool {
        !self.orphan_streams.is_empty()
    }
}

impl Default for HadrStreamCleanup {
    fn default() -> Self {
        Self::new()
    }
}
