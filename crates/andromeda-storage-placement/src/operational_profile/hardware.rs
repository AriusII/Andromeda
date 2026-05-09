use andromeda_hardware::{
    CpuCapabilityClass, CpuProfile, GpuProfile, HardwareArchitecture, HardwareProfile, RamProfile,
    RamSectionBudget, RamSectionRole,
};

use super::constants::{ANALYTICS_RAM_BYTES, HOT_WRITE_RAM_BYTES};

pub(super) fn conservative_hardware(total_ram_bytes: u64) -> HardwareProfile {
    let cpu = CpuProfile::conservative();
    hardware_profile(
        cpu,
        total_ram_bytes,
        [
            (RamSectionRole::Catalog, total_ram_bytes / 8),
            (RamSectionRole::Execution, total_ram_bytes / 8),
            (RamSectionRole::Cache, total_ram_bytes / 4),
            (RamSectionRole::Temp, total_ram_bytes / 8),
            (RamSectionRole::Io, total_ram_bytes / 8),
        ],
        GpuProfile::disabled(),
    )
}

pub(super) fn hot_write_hardware() -> HardwareProfile {
    let cpu = CpuProfile {
        architecture: HardwareArchitecture::Unknown,
        capability_class: CpuCapabilityClass::Scalar64,
        hardware_threads: 2,
    };
    hardware_profile(
        cpu,
        HOT_WRITE_RAM_BYTES,
        [
            (RamSectionRole::Catalog, HOT_WRITE_RAM_BYTES / 8),
            (RamSectionRole::Execution, HOT_WRITE_RAM_BYTES / 4),
            (RamSectionRole::Cache, HOT_WRITE_RAM_BYTES / 4),
            (RamSectionRole::Temp, HOT_WRITE_RAM_BYTES / 8),
            (RamSectionRole::Io, HOT_WRITE_RAM_BYTES / 8),
        ],
        GpuProfile::disabled(),
    )
}

pub(super) fn analytics_hardware() -> HardwareProfile {
    let cpu = CpuProfile {
        architecture: HardwareArchitecture::Unknown,
        capability_class: CpuCapabilityClass::Simd128,
        hardware_threads: 4,
    };
    hardware_profile(
        cpu,
        ANALYTICS_RAM_BYTES,
        [
            (RamSectionRole::Catalog, ANALYTICS_RAM_BYTES / 8),
            (RamSectionRole::Execution, ANALYTICS_RAM_BYTES / 4),
            (RamSectionRole::Cache, ANALYTICS_RAM_BYTES / 4),
            (RamSectionRole::Temp, ANALYTICS_RAM_BYTES / 8),
            (RamSectionRole::Io, ANALYTICS_RAM_BYTES / 8),
        ],
        GpuProfile::batch_analytics_only(),
    )
}

fn hardware_profile<const N: usize>(
    cpu: CpuProfile,
    total_ram_bytes: u64,
    sections: [(RamSectionRole, u64); N],
    gpu: GpuProfile,
) -> HardwareProfile {
    HardwareProfile {
        architecture: cpu.architecture,
        has_simd: cpu.supports_simd(),
        has_direct_io: false,
        cpu,
        ram: ram_profile(total_ram_bytes, sections),
        gpu,
    }
}

fn ram_profile<const N: usize>(
    total_bytes: u64,
    sections: [(RamSectionRole, u64); N],
) -> RamProfile {
    RamProfile::new(
        total_bytes,
        sections
            .into_iter()
            .map(|(role, max_bytes)| RamSectionBudget::new(role, max_bytes))
            .collect(),
    )
}
