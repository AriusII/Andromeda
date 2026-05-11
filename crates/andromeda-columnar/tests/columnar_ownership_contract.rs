use std::num::{NonZeroU16, NonZeroU64};

use andromeda_columnar::{
    ColumnChunkDescriptor, ColumnChunkPruningMetadata, ColumnarAccelerationPolicy,
    ColumnarArtifactDescriptor, ColumnarConsumer, ColumnarLayoutDescriptor,
    ColumnarSegmentDescriptor, ColumnarSnapshotBinding, ColumnarVersionBinding,
};

#[test]
fn columnar_layout_descriptor_is_version_bound_and_advisory() {
    let binding = ColumnarVersionBinding::new(
        NonZeroU64::new(11).unwrap(),
        NonZeroU64::new(7).unwrap(),
        NonZeroU64::new(3).unwrap(),
        Some(NonZeroU64::new(19).unwrap()),
    );
    let descriptor = ColumnarLayoutDescriptor::new(
        ColumnarConsumer::Maps,
        NonZeroU16::new(5).unwrap(),
        binding,
        ColumnarAccelerationPolicy::OptionalAcceleratorWithCpuFallback,
    );

    assert_eq!(descriptor.consumer(), ColumnarConsumer::Maps);
    assert_eq!(descriptor.column_count(), NonZeroU16::new(5).unwrap());
    assert_eq!(descriptor.version_binding(), binding);
    assert!(descriptor.acceleration_policy().has_cpu_fallback());
    assert!(descriptor.requires_explicit_codec_before_persistence());
    assert!(!descriptor.is_source_truth());
    assert!(!descriptor.is_c5_critical_path());
    assert!(descriptor.is_advisory_columnar_artifact());
}

#[test]
fn columnar_layout_can_target_diagnostics_without_claiming_truth() {
    let descriptor = ColumnarLayoutDescriptor::new(
        ColumnarConsumer::OperationalDiagnostics,
        NonZeroU16::MIN,
        ColumnarVersionBinding::new(NonZeroU64::MIN, NonZeroU64::MIN, NonZeroU64::MIN, None),
        ColumnarAccelerationPolicy::CpuOnly,
    );

    assert_eq!(
        descriptor.consumer(),
        ColumnarConsumer::OperationalDiagnostics
    );
    assert!(descriptor.is_advisory_columnar_artifact());
    assert!(!descriptor.is_source_truth());
}

#[test]
fn columnar_segment_descriptor_tracks_snapshot_binding_and_pruning_metadata() {
    let layout = ColumnarLayoutDescriptor::new(
        ColumnarConsumer::Maps,
        NonZeroU16::new(2).unwrap(),
        ColumnarVersionBinding::new(
            NonZeroU64::new(21).unwrap(),
            NonZeroU64::new(34).unwrap(),
            NonZeroU64::new(55).unwrap(),
            Some(NonZeroU64::new(89).unwrap()),
        ),
        ColumnarAccelerationPolicy::CpuOnly,
    );
    let segment = ColumnarSegmentDescriptor::new(
        layout,
        ColumnarSnapshotBinding::new(NonZeroU64::new(144).unwrap()),
        vec![
            ColumnChunkDescriptor::new(
                0,
                NonZeroU64::new(1_024).unwrap(),
                ColumnChunkPruningMetadata::new(true, true, false),
            ),
            ColumnChunkDescriptor::new(
                1,
                NonZeroU64::new(1_024).unwrap(),
                ColumnChunkPruningMetadata::new(false, false, true),
            ),
        ],
    );

    assert_eq!(segment.consumer(), ColumnarConsumer::Maps);
    assert_eq!(segment.chunk_count(), 2);
    assert!(segment.has_pruning_metadata());
    assert!(segment.has_bloom_filters());
    assert!(segment.is_bound_to_source_snapshot(NonZeroU64::new(144).unwrap()));
    assert!(segment.is_advisory_columnar_artifact());
    assert!(!segment.is_source_truth());
}
