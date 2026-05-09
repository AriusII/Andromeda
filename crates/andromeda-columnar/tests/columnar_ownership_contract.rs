use std::num::{NonZeroU16, NonZeroU64};

use andromeda_columnar::{
    ColumnarAccelerationPolicy, ColumnarArtifactDescriptor, ColumnarConsumer,
    ColumnarLayoutDescriptor, ColumnarVersionBinding,
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
