#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PolicyId(u64);

impl PolicyId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl From<u64> for PolicyId {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PolicyVersion([u8; Self::LEN]);

impl PolicyVersion {
    pub const LEN: usize = 32;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; Self::LEN])
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

impl Default for PolicyVersion {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_id_preserves_supplied_identifier() {
        assert_eq!(PolicyId::new(42).get(), 42);
        assert!(PolicyId::default().is_zero());
    }

    #[test]
    fn policy_version_keeps_explicit_bytes() {
        let version = PolicyVersion::new([7; PolicyVersion::LEN]);

        assert_eq!(version.as_bytes(), [7; PolicyVersion::LEN]);
        assert!(!version.is_zero());
        assert!(PolicyVersion::zero().is_zero());
    }
}
