use andromeda_core::ContractHash;

/// Versioning bytes for the policy bundle backing a procedure manifest.
///
/// `ManifestPolicyVersion` is intentionally distinct from `ContractHash` so the
/// type system enforces that the policy version cannot be confused with the
/// contract / structured-object / descriptor hashes. The on-the-wire shape is a
/// 32-byte digest, matching `andromeda-catalog::PolicyVersion` so values can be
/// transcribed without re-hashing.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ManifestPolicyVersion([u8; ContractHash::LEN]);

impl ManifestPolicyVersion {
    pub const LEN: usize = ContractHash::LEN;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; Self::LEN])
    }

    pub const fn test_vector(byte: u8) -> Self {
        Self([byte; Self::LEN])
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

impl Default for ManifestPolicyVersion {
    fn default() -> Self {
        Self::zero()
    }
}

impl core::fmt::Debug for ManifestPolicyVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ManifestPolicyVersion(")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ")")
    }
}
