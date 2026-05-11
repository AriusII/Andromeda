use std::num::NonZeroU64;

use crate::{
    ColumnarAccelerationPolicy, ColumnarArtifactDescriptor, ColumnarConsumer,
    ColumnarLayoutDescriptor, ColumnarVersionBinding,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColumnarSnapshotBinding {
    pub source_snapshot_lsn: NonZeroU64,
}

impl ColumnarSnapshotBinding {
    pub const fn new(source_snapshot_lsn: NonZeroU64) -> Self {
        Self {
            source_snapshot_lsn,
        }
    }

    pub fn is_bound_to(self, source_snapshot_lsn: NonZeroU64) -> bool {
        self.source_snapshot_lsn == source_snapshot_lsn
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColumnChunkPruningMetadata {
    /// Whether this chunk has a stored minimum value.
    pub has_min_value: bool,
    /// Whether this chunk has a stored maximum value.
    pub has_max_value: bool,
    /// Whether this chunk has an associated bloom filter.
    pub has_bloom_filter: bool,
    /// Actual minimum `i64` scalar value for this chunk, when available.
    ///
    /// Advisory only — the optimizer uses this to skip chunks outside a predicate
    /// range. Never used as storage truth.
    pub min_value: Option<i64>,
    /// Actual maximum `i64` scalar value for this chunk, when available.
    ///
    /// Advisory only — the optimizer uses this to skip chunks outside a predicate
    /// range. Never used as storage truth.
    pub max_value: Option<i64>,
}

impl ColumnChunkPruningMetadata {
    /// Construct pruning metadata with flag-only state (min/max values absent).
    pub const fn new(has_min_value: bool, has_max_value: bool, has_bloom_filter: bool) -> Self {
        Self {
            has_min_value,
            has_max_value,
            has_bloom_filter,
            min_value: None,
            max_value: None,
        }
    }

    /// Builder: attach actual i64 min/max values to this metadata.
    ///
    /// Also sets `has_min_value` and `has_max_value` to `true` so that
    /// [`has_min_max`][Self::has_min_max] and [`effective_min_max`][Self::effective_min_max]
    /// remain consistent.
    pub fn with_min_max_values(mut self, min: i64, max: i64) -> Self {
        self.min_value = Some(min);
        self.max_value = Some(max);
        self.has_min_value = true;
        self.has_max_value = true;
        self
    }

    /// Returns `true` when both flag bits indicate min and max are present.
    pub const fn has_min_max(self) -> bool {
        self.has_min_value && self.has_max_value
    }

    /// Returns `true` when this chunk supports any form of predicate pruning.
    pub const fn supports_pruning(self) -> bool {
        self.has_min_max() || self.has_bloom_filter
    }

    /// Returns the `(min, max)` pair when both actual values are available.
    ///
    /// Returns `None` when either value is absent; callers must fall back to a
    /// full scan in that case.
    pub fn effective_min_max(self) -> Option<(i64, i64)> {
        match (self.min_value, self.max_value) {
            (Some(min), Some(max)) => Some((min, max)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColumnChunkDescriptor {
    pub column_ordinal: u16,
    pub row_count: NonZeroU64,
    pub pruning: ColumnChunkPruningMetadata,
}

impl ColumnChunkDescriptor {
    pub const fn new(
        column_ordinal: u16,
        row_count: NonZeroU64,
        pruning: ColumnChunkPruningMetadata,
    ) -> Self {
        Self {
            column_ordinal,
            row_count,
            pruning,
        }
    }

    pub const fn supports_pruning(self) -> bool {
        self.pruning.supports_pruning()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ColumnarSegmentDescriptor {
    layout: ColumnarLayoutDescriptor,
    snapshot_binding: ColumnarSnapshotBinding,
    chunks: Vec<ColumnChunkDescriptor>,
}

impl ColumnarSegmentDescriptor {
    pub fn new(
        layout: ColumnarLayoutDescriptor,
        snapshot_binding: ColumnarSnapshotBinding,
        chunks: Vec<ColumnChunkDescriptor>,
    ) -> Self {
        Self {
            layout,
            snapshot_binding,
            chunks,
        }
    }

    pub const fn layout(&self) -> ColumnarLayoutDescriptor {
        self.layout
    }

    pub const fn snapshot_binding(&self) -> ColumnarSnapshotBinding {
        self.snapshot_binding
    }

    pub fn chunks(&self) -> &[ColumnChunkDescriptor] {
        &self.chunks
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn has_pruning_metadata(&self) -> bool {
        self.chunks
            .iter()
            .copied()
            .any(ColumnChunkDescriptor::supports_pruning)
    }

    pub fn has_bloom_filters(&self) -> bool {
        self.chunks
            .iter()
            .any(|chunk| chunk.pruning.has_bloom_filter)
    }

    pub fn is_bound_to_source_snapshot(&self, source_snapshot_lsn: NonZeroU64) -> bool {
        self.snapshot_binding.is_bound_to(source_snapshot_lsn)
    }
}

impl ColumnarArtifactDescriptor for ColumnarSegmentDescriptor {
    fn consumer(&self) -> ColumnarConsumer {
        self.layout.consumer()
    }

    fn column_count(&self) -> std::num::NonZeroU16 {
        self.layout.column_count()
    }

    fn version_binding(&self) -> ColumnarVersionBinding {
        self.layout.version_binding()
    }

    fn acceleration_policy(&self) -> ColumnarAccelerationPolicy {
        self.layout.acceleration_policy()
    }
}
