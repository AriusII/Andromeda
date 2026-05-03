use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, PipelineClass,
};
use andromeda_observe::{ManifestEventKind, ManifestTrace, TraceId};

use crate::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, CoreIoPlacementRequest, IoPathBudget,
    IoPathClass, IoUseClass, Lsn, PipelineStage, PlacementDecision, SegmentDescriptor, SegmentId,
    StorageIoBudgetScope, StorageTier, StorageWorkloadClass,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatabaseManifest {
    pub database_id: u64,
    pub manifest_version: u64,
    pub snapshot_id: u64,
    pub base_checkpoint_lsn: Lsn,
    pub required_wal_start_lsn: Lsn,
    pub previous_manifest_hash: [u8; 32],
    pub manifest_crc: u32,
}

impl DatabaseManifest {
    pub const fn recovery_floor_lsn(self) -> Lsn {
        self.required_wal_start_lsn
    }

    pub const fn checkpoint_lsn(self) -> Lsn {
        self.base_checkpoint_lsn
    }

    pub const fn can_start_recovery_at(self, lsn: Lsn) -> bool {
        lsn.get() >= self.required_wal_start_lsn.get()
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.database_id == 0 || self.manifest_version == 0 || self.snapshot_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "manifest identity fields must not be zero",
            ));
        }

        if self.required_wal_start_lsn < self.base_checkpoint_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "manifest required WAL start LSN must not precede checkpoint LSN",
            ));
        }

        if self.manifest_crc == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "manifest CRC must not be zero",
            ));
        }

        Ok(())
    }

    pub fn validation_trace(
        &self,
        trace_id: TraceId,
        catalog_version: CatalogVersion,
        manifest_epoch: u64,
    ) -> ManifestTrace {
        ManifestTrace {
            trace_id,
            event: ManifestEventKind::Validation,
            catalog_version,
            manifest_epoch,
            base_checkpoint_lsn: self.base_checkpoint_lsn.get(),
            required_wal_start_lsn: self.required_wal_start_lsn.get(),
            accepted: self.validate().is_ok(),
            reason: "manifest validation checked identity, CRC, and WAL recovery floor".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotSegmentReference {
    pub segment_id: SegmentId,
    pub descriptor_hash: [u8; 32],
}

impl SnapshotSegmentReference {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.segment_id.is_zero() {
            return Err(storage_error("snapshot segment id must not be zero"));
        }
        if self.descriptor_hash == [0; 32] {
            return Err(storage_error(
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
            return Err(storage_error(
                "snapshot publication id must be nonzero and match manifest",
            ));
        }
        if self.publication_epoch == 0 {
            return Err(storage_error("snapshot publication epoch must not be zero"));
        }
        if self.snapshot_descriptor_hash == [0; 32] {
            return Err(storage_error("snapshot descriptor hash must not be zero"));
        }
        if self.segments.is_empty() {
            return Err(storage_error(
                "manifest publication must keep at least one snapshot segment available",
            ));
        }

        for (index, segment) in self.segments.iter().enumerate() {
            segment.validate()?;
            if self.segments[..index]
                .iter()
                .any(|previous| previous.segment_id == segment.segment_id)
            {
                return Err(storage_error(
                    "snapshot publication must not duplicate segment references",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColdSegmentPublicationPlan {
    pub sealed_segment: SegmentDescriptor,
    pub decision: CoreIoPlacementDecision,
}

impl ColdSegmentPublicationPlan {
    pub fn new(
        sealed_segment: SegmentDescriptor,
        policy: &CoreIoPlacementPolicy,
        path_budget: IoPathBudget,
    ) -> AndromedaResult<Self> {
        let segment_bytes = cold_publication_segment_bytes(&sealed_segment)?;
        let decision = policy.plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::ColdPublication,
            StorageIoBudgetScope::Segment {
                bytes: segment_bytes,
            },
            path_budget,
            false,
        ))?;

        validate_cold_segment_publication_boundary(&sealed_segment, &decision)?;

        Ok(Self {
            sealed_segment,
            decision,
        })
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        validate_cold_segment_publication_boundary(&self.sealed_segment, &self.decision)
    }
}

pub fn validate_cold_segment_publication_boundary(
    sealed_segment: &SegmentDescriptor,
    decision: &CoreIoPlacementDecision,
) -> AndromedaResult<()> {
    let expected_placement = PlacementDecision::publish_cold_segment(sealed_segment)?;

    if decision.workload != StorageWorkloadClass::ColdPublication {
        return Err(storage_error(
            "ColdStore segment publication requires the cold publication workload class",
        ));
    }
    if decision.pipeline_class != PipelineClass::BackgroundMaintenance {
        return Err(storage_error(
            "ColdStore segment publication must run on the background publication pipeline",
        ));
    }
    if decision.io_use_class != IoUseClass::ColdSegmentPath {
        return Err(storage_error(
            "ColdStore segment publication requires the cold segment IO budget class",
        ));
    }
    if decision.path_budget.path_class != IoPathClass::ColdPathHdd {
        return Err(storage_error(
            "ColdStore segment publication requires a cold-path IO budget",
        ));
    }
    if decision.gpu_enabled {
        return Err(storage_error(
            "ColdStore segment publication plan must not enable GPU execution",
        ));
    }
    if decision.placement.temperature != expected_placement.temperature
        || decision.placement.target_tier != expected_placement.target_tier
        || decision.placement.pipeline_stage != expected_placement.pipeline_stage
        || decision.placement.read_fallback != expected_placement.read_fallback
        || decision.placement.mutation_allowed != expected_placement.mutation_allowed
    {
        return Err(storage_error(
            "ColdStore segment publication must use the sealed hot segment publication placement",
        ));
    }
    if decision.placement.pipeline_stage != PipelineStage::PublishColdStore
        || decision.placement.target_tier != StorageTier::ColdStore
        || decision.placement.mutation_allowed
    {
        return Err(storage_error(
            "ColdStore segment publication must be immutable and target the cold publication stage",
        ));
    }

    Ok(())
}

fn cold_publication_segment_bytes(sealed_segment: &SegmentDescriptor) -> AndromedaResult<u64> {
    sealed_segment.validate()?;
    u64::from(sealed_segment.page_count)
        .checked_mul(u64::from(sealed_segment.page_size.bytes()))
        .ok_or_else(|| storage_error("cold publication segment byte size overflows u64"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotAvailabilityContract {
    pub published_snapshot_id: u64,
    pub available_snapshot_ids: Vec<u64>,
}

impl SnapshotAvailabilityContract {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.published_snapshot_id == 0 {
            return Err(storage_error("published snapshot id must not be zero"));
        }
        if self.available_snapshot_ids.is_empty() {
            return Err(storage_error(
                "at least one valid snapshot must remain available",
            ));
        }
        if !self
            .available_snapshot_ids
            .contains(&self.published_snapshot_id)
        {
            return Err(storage_error(
                "published snapshot must remain in the available snapshot set",
            ));
        }
        if self.available_snapshot_ids.contains(&0) {
            return Err(storage_error("available snapshot ids must not be zero"));
        }
        Ok(())
    }
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_observe::{EventCorrelation, EventEnvelope, EventId, TraceEvent};

    fn valid_manifest() -> DatabaseManifest {
        DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(10),
            required_wal_start_lsn: Lsn::new(11),
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        }
    }

    #[test]
    fn manifest_exposes_recovery_floor() {
        let manifest = valid_manifest();

        assert_eq!(manifest.checkpoint_lsn(), Lsn::new(10));
        assert_eq!(manifest.recovery_floor_lsn(), Lsn::new(11));
        assert!(!manifest.can_start_recovery_at(Lsn::new(10)));
        assert!(manifest.can_start_recovery_at(Lsn::new(11)));
    }

    #[test]
    fn manifest_rejects_wal_start_before_checkpoint() {
        let mut manifest = valid_manifest();
        manifest.required_wal_start_lsn = Lsn::new(9);

        assert_eq!(
            manifest.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn manifest_validation_trace_carries_queryable_catalog_and_wal_floor() {
        let manifest = valid_manifest();
        let trace = manifest.validation_trace(TraceId::new(40), CatalogVersion::new(41), 42);

        let envelope = EventEnvelope::new(
            EventId::new(43),
            EventCorrelation {
                catalog_version: Some(CatalogVersion::new(41)),
                ..EventCorrelation::empty()
            },
            TraceEvent::Manifest(trace),
        )
        .expect("manifest validation trace is acceptable forensic evidence");

        assert_eq!(
            envelope.correlation.catalog_version,
            Some(CatalogVersion::new(41))
        );
        assert_eq!(
            match envelope.event {
                TraceEvent::Manifest(trace) => trace.required_wal_start_lsn,
                _ => 0,
            },
            11
        );
    }

    #[test]
    fn snapshot_publication_requires_segments_and_matching_manifest() {
        let publication = DatabaseSnapshotPublication {
            manifest: valid_manifest(),
            snapshot_id: 3,
            publication_epoch: 1,
            snapshot_descriptor_hash: [4; 32],
            segments: vec![SnapshotSegmentReference {
                segment_id: SegmentId::new(5),
                descriptor_hash: [6; 32],
            }],
        };
        assert!(publication.validate().is_ok());

        let mut empty = publication.clone();
        empty.segments.clear();
        assert_eq!(
            empty.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );

        let mut mismatch = publication;
        mismatch.snapshot_id = 9;
        assert_eq!(
            mismatch.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn snapshot_availability_requires_one_valid_snapshot_remaining() {
        let contract = SnapshotAvailabilityContract {
            published_snapshot_id: 3,
            available_snapshot_ids: vec![3],
        };
        assert!(contract.validate().is_ok());

        let empty = SnapshotAvailabilityContract {
            published_snapshot_id: 3,
            available_snapshot_ids: Vec::new(),
        };
        assert_eq!(
            empty.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );

        let missing_published = SnapshotAvailabilityContract {
            published_snapshot_id: 3,
            available_snapshot_ids: vec![4],
        };
        assert_eq!(
            missing_published.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
