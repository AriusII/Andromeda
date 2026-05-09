use andromeda_core::AndromedaResult;
use andromeda_segment::SegmentId;

use crate::{DatabaseManifest, manifest_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotSegmentReference {
    pub segment_id: SegmentId,
    pub descriptor_hash: [u8; 32],
}

impl SnapshotSegmentReference {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.segment_id.is_zero() {
            return Err(manifest_error("snapshot segment id must not be zero"));
        }
        if self.descriptor_hash == [0; 32] {
            return Err(manifest_error(
                "snapshot segment descriptor hash must not be zero",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseSnapshotPublication {
    pub manifest: DatabaseManifest,
    pub snapshot_id: u64,
    pub publication_epoch: u64,
    pub snapshot_descriptor_hash: [u8; 32],
    pub segments: Vec<SnapshotSegmentReference>,
}

impl DatabaseSnapshotPublication {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.manifest.validate()?;
        if self.snapshot_id == 0 || self.snapshot_id != self.manifest.snapshot_id {
            return Err(manifest_error(
                "snapshot publication id must be nonzero and match manifest",
            ));
        }
        if self.publication_epoch == 0 {
            return Err(manifest_error(
                "snapshot publication epoch must not be zero",
            ));
        }
        if self.snapshot_descriptor_hash == [0; 32] {
            return Err(manifest_error("snapshot descriptor hash must not be zero"));
        }
        if self.segments.is_empty() {
            return Err(manifest_error(
                "manifest publication must keep at least one snapshot segment available",
            ));
        }

        for (index, segment) in self.segments.iter().enumerate() {
            segment.validate()?;
            if self.segments[..index]
                .iter()
                .any(|previous| previous.segment_id == segment.segment_id)
            {
                return Err(manifest_error(
                    "snapshot publication must not duplicate segment references",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotAvailabilityContract {
    pub published_snapshot_id: u64,
    pub available_snapshot_ids: Vec<u64>,
}

impl SnapshotAvailabilityContract {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.published_snapshot_id == 0 {
            return Err(manifest_error("published snapshot id must not be zero"));
        }
        if self.available_snapshot_ids.is_empty() {
            return Err(manifest_error(
                "at least one valid snapshot must remain available",
            ));
        }
        if !self
            .available_snapshot_ids
            .contains(&self.published_snapshot_id)
        {
            return Err(manifest_error(
                "published snapshot must remain in the available snapshot set",
            ));
        }
        if self.available_snapshot_ids.contains(&0) {
            return Err(manifest_error("available snapshot ids must not be zero"));
        }
        Ok(())
    }
}
