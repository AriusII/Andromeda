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
    pub has_min_value: bool,
    pub has_max_value: bool,
    pub has_bloom_filter: bool,
}

impl ColumnChunkPruningMetadata {
    pub const fn new(has_min_value: bool, has_max_value: bool, has_bloom_filter: bool) -> Self {
        Self {
            has_min_value,
            has_max_value,
            has_bloom_filter,
        }
    }

    pub const fn has_min_max(self) -> bool {
        self.has_min_value && self.has_max_value
    }

    pub const fn supports_pruning(self) -> bool {
        self.has_min_max() || self.has_bloom_filter
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
