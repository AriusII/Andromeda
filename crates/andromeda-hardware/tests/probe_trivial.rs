// Probe test — checks if andromeda-hardware integration tests can run at all.
use andromeda_hardware::CpuKernelKind;

#[test]
fn probe_trivial() {
    let _ = CpuKernelKind::Hash;
}
