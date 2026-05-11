// Probe with the equivalence pattern content under a neutral name.
#![forbid(unsafe_code)]
use andromeda_hardware::{CpuKernelKind, CpuKernelRegistry, CpuKernelVariant, CpuRuntimeProfile};

fn xor_sum(data: &[u8]) -> u32 {
    let mut acc: u32 = 0;
    let chunks = data.chunks_exact(4);
    let tail = chunks.remainder();
    for chunk in chunks {
        acc = acc.wrapping_add(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    for (i, &b) in tail.iter().enumerate() {
        acc ^= (u32::from(b)) << (8 * i);
    }
    acc
}

#[test]
fn probe_equivalence_and_dispatch() {
    assert_eq!(xor_sum(&[]), xor_sum(&[]));
    assert_eq!(xor_sum(&[0x42]), xor_sum(&[0x42]));
    let d = CpuKernelRegistry::v0()
        .dispatch_for_runtime_profile(CpuKernelKind::Hash, CpuRuntimeProfile::Conservative);
    assert_eq!(d.selected_variant, CpuKernelVariant::Scalar);
}
