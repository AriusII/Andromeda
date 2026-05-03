#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareArchitecture {
    X64,
    Arm64,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareProfile {
    pub architecture: HardwareArchitecture,
    pub has_simd: bool,
    pub has_direct_io: bool,
}

impl HardwareProfile {
    pub const fn conservative() -> Self {
        Self {
            architecture: HardwareArchitecture::Unknown,
            has_simd: false,
            has_direct_io: false,
        }
    }
}

impl Default for HardwareProfile {
    fn default() -> Self {
        Self::conservative()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceBudget {
    pub max_memory_bytes: u64,
    pub max_temp_bytes: u64,
    pub max_streams: u32,
}

impl ResourceBudget {
    pub const fn new(max_memory_bytes: u64, max_temp_bytes: u64, max_streams: u32) -> Self {
        Self {
            max_memory_bytes,
            max_temp_bytes,
            max_streams,
        }
    }
}
