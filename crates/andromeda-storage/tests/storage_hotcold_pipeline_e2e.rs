use andromeda_error::AndromedaErrorKind;
use andromeda_hardware::PipelineClass;
use andromeda_observe::{
    CriticalDecisionKind, EventCorrelation, EventEnvelope, EventId, IoPipelineStage,
    IoPlacementDecisionTrace, IoStorageTier, ManifestEventKind, TraceEvent, TraceId,
};
use andromeda_storage::{
    AllocationId, DataTemperature, DatabaseManifest, DatabaseManifestTraceExt,
    DatabaseSnapshotPublication, ExtentId, Lsn, ObjectId, PageId, PageSize, PipelineStage,
    PlacementDecision, ReadFallbackPolicy, SegmentDescriptor, SegmentHeader, SegmentId,
    SegmentMutation, SegmentState, SegmentTrailer, SnapshotAvailabilityContract,
    SnapshotSegmentReference, StorageTier,
};
use andromeda_types::CatalogVersion;

fn segment(state: SegmentState) -> SegmentDescriptor {
    let header = SegmentHeader {
        magic: SegmentHeader::MAGIC,
        format_version: SegmentHeader::FORMAT_VERSION_V0,
        segment_id: SegmentId::new(600),
        object_id: ObjectId::new(601),
        allocation_id: AllocationId::new(602),
        first_page_id: PageId::new(10_000),
        page_count: 16,
        min_page_lsn: Lsn::new(603),
        max_page_lsn: Lsn::new(604),
        header_crc: 605,
    };

    SegmentDescriptor {
        segment_id: header.segment_id,
        object_id: header.object_id,
        allocation_id: header.allocation_id,
        first_extent_id: ExtentId::new(606),
        extent_count: 2,
        first_page_id: header.first_page_id,
        page_count: header.page_count,
        page_size: PageSize::KiB16,
        min_page_lsn: header.min_page_lsn,
        max_page_lsn: header.max_page_lsn,
        snapshot_id: match state {
            SegmentState::BuildingHotSnapshot => None,
            SegmentState::Sealed | SegmentState::PublishedCold => Some(607),
        },
        state,
        header,
        trailer: SegmentTrailer {
            segment_payload_crc64: 608,
            segment_hash: [7; 32],
            trailer_crc: 609,
        },
    }
}

#[test]
fn append_pipeline_places_mutable_work_in_hotstore_not_coldstore() {
    let decision = PlacementDecision::append(DataTemperature::RamWorkingSet).unwrap();

    assert_eq!(decision.target_tier, StorageTier::HotStore);
    assert_eq!(decision.pipeline_stage, PipelineStage::AppendHotStore);
    assert_eq!(
        decision.pipeline_stage.source_tier(),
        Some(StorageTier::Ram)
    );
    assert_eq!(
        decision.pipeline_stage.destination_tier(),
        Some(StorageTier::HotStore)
    );
    assert!(decision.mutation_allowed);
    assert!(decision.validate().is_ok());

    assert_eq!(
        PlacementDecision::append(DataTemperature::Cold)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn hot_and_cold_read_classification_is_explicit() {
    let hot = PlacementDecision::read(DataTemperature::Hot);
    assert_eq!(hot.target_tier, StorageTier::HotStore);
    assert_eq!(hot.read_fallback, ReadFallbackPolicy::HotThenCold);
    assert!(hot.read_fallback.allows_tier(StorageTier::ColdStore));
    assert!(hot.validate().is_ok());

    let cold = PlacementDecision::read(DataTemperature::Cold);
    assert_eq!(cold.target_tier, StorageTier::ColdStore);
    assert_eq!(cold.read_fallback, ReadFallbackPolicy::ColdOnly);
    assert!(!cold.mutation_allowed);
    assert!(cold.validate().is_ok());
}

#[test]
fn cold_publication_requires_hotstore_seal_and_stays_immutable() {
    let building = segment(SegmentState::BuildingHotSnapshot);
    let seal = PlacementDecision::seal_hot_segment(&building).unwrap();
    assert_eq!(seal.target_tier, StorageTier::HotStore);
    assert_eq!(seal.pipeline_stage, PipelineStage::SealHotStoreSegment);
    assert!(!seal.mutation_allowed);

    let sealed = segment(SegmentState::Sealed);
    let cold = PlacementDecision::publish_cold_segment(&sealed).unwrap();
    assert_eq!(cold.target_tier, StorageTier::ColdStore);
    assert_eq!(cold.pipeline_stage, PipelineStage::PublishColdStore);
    assert_eq!(cold.read_fallback, ReadFallbackPolicy::ColdOnly);
    assert!(!cold.mutation_allowed);
    assert!(cold.validate().is_ok());

    let published = segment(SegmentState::PublishedCold);
    assert_eq!(
        PlacementDecision::publish_cold_segment(&published)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
    assert_eq!(
        PlacementDecision::mutation(
            &published,
            SegmentMutation::AppendExtent,
            DataTemperature::Hot
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn cold_publication_carries_manifest_availability_and_trace_evidence() {
    let sealed = segment(SegmentState::Sealed);
    let placement = PlacementDecision::publish_cold_segment(&sealed).unwrap();
    assert_eq!(placement.pipeline_stage, PipelineStage::PublishColdStore);
    assert_eq!(placement.target_tier, StorageTier::ColdStore);
    assert!(!placement.mutation_allowed);

    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: sealed.snapshot_id.unwrap(),
        base_checkpoint_lsn: sealed.min_page_lsn,
        required_wal_start_lsn: sealed.max_page_lsn,
        previous_manifest_hash: [3; 32],
        manifest_crc: 4,
    };
    let publication = DatabaseSnapshotPublication {
        manifest,
        snapshot_id: sealed.snapshot_id.unwrap(),
        publication_epoch: 5,
        snapshot_descriptor_hash: [6; 32],
        segments: vec![SnapshotSegmentReference {
            segment_id: sealed.segment_id,
            descriptor_hash: sealed.trailer.segment_hash,
        }],
    };
    let availability = SnapshotAvailabilityContract {
        published_snapshot_id: sealed.snapshot_id.unwrap(),
        available_snapshot_ids: vec![sealed.snapshot_id.unwrap()],
    };

    assert!(publication.validate().is_ok());
    assert!(availability.validate().is_ok());
    assert!(manifest.can_start_recovery_at(sealed.max_page_lsn));

    let placement_envelope = EventEnvelope::new(
        EventId::new(701),
        EventCorrelation::empty(),
        TraceEvent::IoPlacementDecision(
            IoPlacementDecisionTrace::accepted(
                TraceId::new(702),
                PipelineClass::BackgroundMaintenance,
                IoPipelineStage::Cold,
                IoStorageTier::Cold,
                placement.reason,
            )
            .expect("placement helper requires explicit reason evidence"),
        ),
    )
    .expect("ColdStore publication placement must produce accepted IO placement evidence");
    assert_eq!(
        placement_envelope.event.kind(),
        CriticalDecisionKind::IoPlacementDecision
    );

    let manifest_trace = manifest.validation_trace(
        TraceId::new(703),
        CatalogVersion::new(9),
        publication.publication_epoch,
    );
    assert_eq!(manifest_trace.event, ManifestEventKind::Validation);
    let manifest_envelope = EventEnvelope::new(
        EventId::new(704),
        EventCorrelation {
            catalog_version: Some(CatalogVersion::new(9)),
            ..EventCorrelation::empty()
        },
        TraceEvent::Manifest(manifest_trace),
    )
    .expect("manifest publication must carry matching catalog and WAL anchor evidence");
    assert_eq!(
        manifest_envelope.event.kind(),
        CriticalDecisionKind::ManifestValidation
    );
}
