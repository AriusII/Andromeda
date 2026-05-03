use andromeda_core::{HardwareArchitecture, HardwareProfile, ResourceBudget};

#[test]
fn conservative_hardware_profile_keeps_accelerators_out_of_required_path() {
    let profile = HardwareProfile::conservative();

    assert_eq!(profile.architecture, HardwareArchitecture::Unknown);
    assert!(!profile.has_simd);
    assert!(!profile.has_direct_io);
    assert_eq!(HardwareProfile::default(), profile);
}

#[test]
fn resource_budget_preserves_explicit_ram_temp_and_stream_limits() {
    let budget = ResourceBudget::new(64 * 1024 * 1024, 256 * 1024 * 1024, 8);

    assert_eq!(budget.max_memory_bytes, 64 * 1024 * 1024);
    assert_eq!(budget.max_temp_bytes, 256 * 1024 * 1024);
    assert_eq!(budget.max_streams, 8);
}
