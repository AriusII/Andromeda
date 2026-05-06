use super::CertificateFingerprint;

/// Unique identifier for a principal (user or service).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrincipalId(u64);

impl PrincipalId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Derive a stable, non-zero principal id from certificate fingerprint.
    ///
    /// SHA-256 fingerprints follow the core derivation rule (first 8 hex chars).
    /// Non-SHA fingerprints use legacy deterministic byte folding.
    pub fn from_certificate_fingerprint(
        fingerprint: &CertificateFingerprint,
    ) -> crate::AndromedaResult<Self> {
        if fingerprint.as_str().trim().is_empty() {
            return Err(crate::AndromedaError::new(
                crate::AndromedaErrorKind::Security,
                "certificate fingerprint must not be empty",
            ));
        }

        let value = if fingerprint.is_valid_sha256() {
            let hex_part = &fingerprint.as_str()[..8];
            u64::from_str_radix(hex_part, 16).map_err(|_| {
                crate::AndromedaError::new(
                    crate::AndromedaErrorKind::Security,
                    "certificate fingerprint hex parsing failed",
                )
            })?
        } else {
            fingerprint
                .as_str()
                .as_bytes()
                .iter()
                .fold(0u64, |acc, &byte| {
                    acc.wrapping_mul(31).wrapping_add(byte as u64)
                })
        };

        Ok(Self::new(if value == 0 { 1 } else { value }))
    }
}

impl From<u64> for PrincipalId {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for PrincipalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PrincipalId({})", self.0)
    }
}
