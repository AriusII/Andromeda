use andromeda_core::PipelineClass;
use andromeda_storage::{
    AllocationId, CoreIoPlacementPolicy, CoreIoPlacementRequest, ExtentId, Lsn, ObjectId,
    OperationalProfile, PageId, PageSize, PipelineStage, PublishedColdSegment, SegmentDescriptor,
    SegmentHeader, SegmentId, SegmentMutation, SegmentState, SegmentTrailer, StorageIoBudgetScope,
    StorageTier, StorageWorkloadClass,
};

const STORAGE_CRITICAL_PATH_SOURCES: &[(&str, &str)] = &[
    ("wal", include_str!("../src/wal.rs")),
    ("wal_codec", include_str!("../src/wal_codec.rs")),
    ("recovery", include_str!("../src/recovery.rs")),
    ("segment", include_str!("../src/segment.rs")),
    ("cold_store", include_str!("../src/cold_store.rs")),
];

fn descriptor(state: SegmentState) -> SegmentDescriptor {
    let header = SegmentHeader {
        magic: SegmentHeader::MAGIC,
        format_version: SegmentHeader::FORMAT_VERSION_V0,
        segment_id: SegmentId::new(401),
        object_id: ObjectId::new(402),
        allocation_id: AllocationId::new(403),
        first_page_id: PageId::new(10_000),
        page_count: 16,
        min_page_lsn: Lsn::new(500),
        max_page_lsn: Lsn::new(600),
        header_crc: 701,
    };

    SegmentDescriptor {
        segment_id: header.segment_id,
        object_id: header.object_id,
        allocation_id: header.allocation_id,
        first_extent_id: ExtentId::new(404),
        extent_count: 2,
        first_page_id: header.first_page_id,
        page_count: header.page_count,
        page_size: PageSize::KiB16,
        min_page_lsn: header.min_page_lsn,
        max_page_lsn: header.max_page_lsn,
        snapshot_id: (state == SegmentState::PublishedCold).then_some(7),
        state,
        header,
        trailer: SegmentTrailer {
            segment_payload_crc64: 702,
            segment_hash: [7; 32],
            trailer_crc: 703,
        },
    }
}

#[test]
fn building_hot_snapshot_remains_mutable_before_seal_or_publication() {
    let descriptor = descriptor(SegmentState::BuildingHotSnapshot);

    assert!(descriptor.validate().is_ok());
    for mutation in [
        SegmentMutation::AppendExtent,
        SegmentMutation::UpdatePageInPlace,
        SegmentMutation::SplitSegment,
    ] {
        assert!(
            descriptor.validate_mutation(mutation).is_ok(),
            "building hot snapshot should accept {mutation:?} before seal/publication"
        );
    }
}

#[test]
fn sealed_segment_blocks_in_place_drift_but_allows_append_extent() {
    let descriptor = descriptor(SegmentState::Sealed);

    assert!(descriptor
        .validate_mutation(SegmentMutation::AppendExtent)
        .is_ok());
    assert!(descriptor
        .validate_mutation(SegmentMutation::UpdatePageInPlace)
        .is_err());
    assert!(descriptor
        .validate_mutation(SegmentMutation::SplitSegment)
        .is_err());
}

#[test]
fn cold_store_rejects_all_mutations_after_publication() {
    let descriptor = descriptor(SegmentState::PublishedCold);
    let cold_segment = PublishedColdSegment::new(descriptor).unwrap();

    assert!(cold_segment.validate_immutable().is_ok());
    for mutation in [
        SegmentMutation::AppendExtent,
        SegmentMutation::UpdatePageInPlace,
        SegmentMutation::SplitSegment,
    ] {
        assert!(descriptor.validate_mutation(mutation).is_err());
        assert!(cold_segment.reject_mutation(mutation).is_err());
    }
}

#[test]
fn commit_wal_rollback_and_recovery_workloads_stay_on_hotstore_path() {
    let profile = OperationalProfile::hot_write();
    profile
        .validate()
        .expect("hot-write operational profile must validate before core IO planning");
    let policy = CoreIoPlacementPolicy::new(profile.hardware.clone(), profile.workflow.thresholds);
    let segment_scope = StorageIoBudgetScope::Segment {
        bytes: profile.workflow.segment_budget.segment_bytes,
    };

    for (workload, expected_pipeline) in [
        (StorageWorkloadClass::Commit, PipelineClass::Commit),
        (StorageWorkloadClass::WalAppend, PipelineClass::WalAppend),
        (StorageWorkloadClass::Rollback, PipelineClass::Rollback),
        (StorageWorkloadClass::Recovery, PipelineClass::Recovery),
    ] {
        let decision = policy
            .plan(CoreIoPlacementRequest::new(
                workload,
                segment_scope,
                profile.workflow.segment_budget.path_budget,
                false,
            ))
            .expect("critical WAL/recovery workload must be admitted to the hot path");

        assert_eq!(decision.pipeline_class, expected_pipeline);
        assert_eq!(decision.placement.target_tier, StorageTier::HotStore);
        assert_eq!(
            decision.placement.pipeline_stage,
            PipelineStage::AppendHotStore
        );
        assert!(decision.placement.mutation_allowed);
        assert!(!decision.gpu_enabled);
    }
}

#[test]
fn cold_publication_workload_stays_on_immutable_coldstore_path() {
    let profile = OperationalProfile::cold_archive();
    profile
        .validate()
        .expect("cold archive operational profile must validate before cold publication planning");
    let policy = CoreIoPlacementPolicy::new(profile.hardware.clone(), profile.workflow.thresholds);

    let decision = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::ColdPublication,
            StorageIoBudgetScope::Segment {
                bytes: profile.workflow.segment_budget.segment_bytes,
            },
            profile.workflow.segment_budget.path_budget,
            false,
        ))
        .expect("cold publication should be admitted only to immutable ColdStore");

    assert_eq!(
        decision.pipeline_class,
        PipelineClass::BackgroundMaintenance
    );
    assert_eq!(decision.placement.target_tier, StorageTier::ColdStore);
    assert_eq!(
        decision.placement.pipeline_stage,
        PipelineStage::PublishColdStore
    );
    assert!(!decision.placement.mutation_allowed);
    assert!(!decision.gpu_enabled);
}

#[test]
fn storage_critical_path_has_no_gpu_sql_grpc_runtime_json_or_unsafe_surface() {
    for (name, source) in STORAGE_CRITICAL_PATH_SOURCES {
        let normalized = source.to_ascii_lowercase();

        for forbidden in [
            "gpu",
            "grpc",
            "tonic",
            "serde_json",
            "runtime json",
            "select *",
            "unsafe",
        ] {
            assert!(
                !contains_forbidden_token(&normalized, forbidden),
                "{name} critical path must not contain forbidden token {forbidden:?}"
            );
        }
    }
}

#[test]
fn doctrine_scan_helper_uses_token_boundaries_without_weakening_forbidden_list() {
    assert!(contains_forbidden_token("call unsafe now", "unsafe"));
    assert!(contains_forbidden_token("serde_json::value", "serde_json"));
    assert!(contains_forbidden_token("select * from t", "select *"));

    assert!(!contains_forbidden_token("safely_unsafe_name", "unsafe"));
    assert!(!contains_forbidden_token("grouped by capability", "gpu"));
    assert!(!contains_forbidden_token("tonicity", "tonic"));
}

fn contains_forbidden_token(source: &str, forbidden: &str) -> bool {
    if forbidden.contains(' ') || forbidden.contains('*') || forbidden.contains('_') {
        return source.contains(forbidden);
    }

    source
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == forbidden)
}
