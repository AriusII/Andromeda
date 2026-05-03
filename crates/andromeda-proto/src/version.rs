#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

impl ProtocolVersion {
    pub const V1: Self = Self { major: 1, minor: 0 };

    pub const fn is_supported(self) -> bool {
        self.major == Self::V1.major
    }
}
