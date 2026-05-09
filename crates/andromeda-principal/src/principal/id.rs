use super::CertificateFingerprint;

/// Unique identifier for a principal (user or service).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrincipalId(u64);

impl PrincipalId {
    /// Constructs the identifier without applying active-principal validation.
    ///
    /// Zero is preserved for compatibility with tests, sentinels, and
    /// validation paths. Runtime principal constructors should reject zero
    /// where a bound, active principal is required.
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
    /// Non-SHA fingerprints use historical deterministic byte folding. If the
    /// deterministic derivation yields zero, the returned audit identifier is
    /// mapped to `1` so certificate-derived principal IDs are never zero.
    pub fn from_certificate_fingerprint(
        fingerprint: &CertificateFingerprint,
    ) -> andromeda_error::AndromedaResult<Self> {
        if fingerprint.as_str().trim().is_empty() {
            return Err(andromeda_error::AndromedaError::new(
                andromeda_error::AndromedaErrorKind::Security,
                "certificate fingerprint must not be empty",
            ));
        }

        let value = if fingerprint.is_valid_sha256() {
            let hex_part = &fingerprint.as_str()[..8];
            u64::from_str_radix(hex_part, 16).map_err(|_| {
                andromeda_error::AndromedaError::new(
                    andromeda_error::AndromedaErrorKind::Security,
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

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

    fn fingerprint_evidence(value: &str) -> AndromedaResult<CertificateFingerprint> {
        CertificateFingerprint::new(value).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Security,
                "test fingerprint evidence should be accepted",
            )
        })
    }

    #[test]
    fn sha256_fingerprint_uses_first_eight_hex_chars() -> AndromedaResult<()> {
        let fingerprint = fingerprint_evidence(
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
        )?;

        let principal_id = PrincipalId::from_certificate_fingerprint(&fingerprint)?;

        assert_eq!(principal_id.get(), 0xabcdef01);
        Ok(())
    }

    #[test]
    fn legacy_fingerprint_uses_stable_byte_folding() -> AndromedaResult<()> {
        let fingerprint = fingerprint_evidence("legacy-fingerprint")?;

        let principal_id = PrincipalId::from_certificate_fingerprint(&fingerprint)?;

        let expected = fingerprint
            .as_str()
            .as_bytes()
            .iter()
            .fold(0u64, |acc, &byte| {
                acc.wrapping_mul(31).wrapping_add(byte as u64)
            });
        assert_eq!(principal_id.get(), expected);
        Ok(())
    }

    #[test]
    fn empty_fingerprint_is_security_error() {
        let fingerprint = CertificateFingerprint::new_unchecked("");

        let error = PrincipalId::from_certificate_fingerprint(&fingerprint)
            .expect_err("empty fingerprint evidence must fail");

        assert_eq!(error.kind(), AndromedaErrorKind::Security);
    }

    #[test]
    fn certificate_derived_principal_id_is_never_zero() -> AndromedaResult<()> {
        let fingerprint = fingerprint_evidence(
            "00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        )?;

        let principal_id = PrincipalId::from_certificate_fingerprint(&fingerprint)?;

        assert_eq!(principal_id.get(), 1);
        assert!(!principal_id.is_zero());
        Ok(())
    }
}
