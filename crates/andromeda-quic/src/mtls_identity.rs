//! mTLS certificate identity extraction and validation.
//!
//! This module provides the contract surface for extracting and binding mTLS
//! certificate identities to QUIC sessions. The default crate remains
//! runtime-free; concrete Quinn extraction lives in
//! `andromeda-quic-runtime-quinn`.

use andromeda_digest::sha256;
use andromeda_error::AndromedaResult;
use andromeda_principal::SurfaceScope;

/// Convert a QUIC `SurfacePlane` to its required [`SurfaceScope`].
///
/// Each plane enforces a certificate scope policy:
/// - Application plane → Application scope
/// - Administration plane → Administration scope
/// - HA/DR plane → Cluster scope
/// - Monitoring plane → MonitoringAgent scope
///
/// This invariant is checked at session construction: a certificate
/// with surface scope "Administration" is rejected on the Application listener.
pub const fn plane_to_required_surface_scope(plane: crate::SurfacePlane) -> SurfaceScope {
    match plane {
        crate::SurfacePlane::Application => SurfaceScope::Application,
        crate::SurfacePlane::Administration => SurfaceScope::Administration,
        crate::SurfacePlane::HighAvailability => SurfaceScope::Cluster,
        crate::SurfacePlane::Monitoring => SurfaceScope::MonitoringAgent,
    }
}

/// Raw X.509 certificate bytes extracted from a QUIC connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCertificate {
    /// DER-encoded X.509 certificate bytes.
    pub der_bytes: Vec<u8>,
}

impl RawCertificate {
    /// Construct from DER-encoded bytes.
    pub fn new(der_bytes: Vec<u8>) -> Self {
        Self { der_bytes }
    }

    /// Length of the DER encoding.
    pub fn len(&self) -> usize {
        self.der_bytes.len()
    }

    /// True if no certificate is present.
    pub fn is_empty(&self) -> bool {
        self.der_bytes.is_empty()
    }

    /// SHA-256 fingerprint of the DER-encoded certificate as lowercase hex.
    pub fn fingerprint_sha256_hex(&self) -> String {
        hex_sha256(&self.der_bytes)
    }

    /// Build identity evidence directly from a raw DER certificate.
    ///
    /// This is the Quinn fallback path used when the transport exposes the
    /// peer certificate bytes but no X.509 parser hook is available. The
    /// fingerprint is the authorization key; the subject remains stable,
    /// non-secret evidence derived from that fingerprint.
    pub fn to_certificate_identity(
        &self,
        required_scope: SurfaceScope,
    ) -> AndromedaResult<andromeda_principal::CertificateIdentity> {
        if self.is_empty() {
            return Err(andromeda_error::AndromedaError::new(
                andromeda_error::AndromedaErrorKind::Security,
                "peer certificate cannot be empty",
            ));
        }

        let fingerprint = self.fingerprint_sha256_hex();
        let subject = format!("sha256:{}", &fingerprint[..16]);
        andromeda_principal::CertificateIdentity::new(fingerprint, subject, required_scope)
    }
}

/// X.509 certificate fields extracted for identity and policy validation.
///
/// ## Invariants
///
/// - `subject_cn` is non-empty and trimmed.
/// - `fingerprint_sha256` is a valid hex string of length 64 (256 bits).
/// - Fingerprint is stable across the certificate's lifetime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCertificate {
    /// CommonName from the certificate Subject field.
    pub subject_cn: String,
    /// DNS names from SubjectAltName extension (if present).
    pub san_dns_names: Vec<String>,
    /// SHA256 fingerprint as hex string (64 characters).
    pub fingerprint_sha256: String,
    /// CommonName from the certificate Issuer field (optional).
    pub issuer_cn: Option<String>,
}

impl ParsedCertificate {
    /// Create a parsed certificate entry.
    pub fn new(
        subject_cn: String,
        fingerprint_sha256: String,
        issuer_cn: Option<String>,
    ) -> AndromedaResult<Self> {
        if subject_cn.trim().is_empty() {
            return Err(andromeda_error::AndromedaError::new(
                andromeda_error::AndromedaErrorKind::Security,
                "certificate subject CN cannot be empty",
            ));
        }
        if fingerprint_sha256.len() != 64 {
            return Err(andromeda_error::AndromedaError::new(
                andromeda_error::AndromedaErrorKind::Security,
                "certificate fingerprint must be 64 hex characters (SHA256)",
            ));
        }
        // Validate hex encoding.
        if !fingerprint_sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(andromeda_error::AndromedaError::new(
                andromeda_error::AndromedaErrorKind::Security,
                "certificate fingerprint must be valid hex",
            ));
        }

        Ok(Self {
            subject_cn,
            san_dns_names: Vec::new(),
            fingerprint_sha256,
            issuer_cn,
        })
    }

    /// Add a SAN DNS name to the parsed certificate.
    pub fn add_san_dns_name(mut self, name: String) -> Self {
        if !name.trim().is_empty() && !self.san_dns_names.contains(&name) {
            self.san_dns_names.push(name);
        }
        self
    }

    /// Converts the parsed certificate into the required surface scope.
    ///
    /// Returns `Err` if the certificate does not meet scope requirements.
    pub fn to_certificate_identity(
        self,
        required_scope: SurfaceScope,
    ) -> AndromedaResult<andromeda_principal::CertificateIdentity> {
        let subject = self.subject_cn.clone();
        andromeda_principal::CertificateIdentity::new(
            self.fingerprint_sha256,
            subject,
            required_scope,
        )
    }
}

/// Validate that a fingerprint is a valid SHA256 hex string.
///
/// Returns `Err` if the string is not exactly 64 hex characters.
pub fn validate_fingerprint(fp: &str) -> AndromedaResult<()> {
    if fp.len() != 64 {
        return Err(andromeda_error::AndromedaError::new(
            andromeda_error::AndromedaErrorKind::Security,
            "certificate fingerprint must be 64 hex characters",
        ));
    }
    if !fp.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(andromeda_error::AndromedaError::new(
            andromeda_error::AndromedaErrorKind::Security,
            "certificate fingerprint must be valid hex",
        ));
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = sha256(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_certificate_len_and_empty() {
        let raw = RawCertificate::new(vec![0x30, 0x82]); // minimal DER ASN.1
        assert_eq!(raw.len(), 2);
        assert!(!raw.is_empty());

        let empty = RawCertificate::new(vec![]);
        assert!(empty.is_empty());
    }

    #[test]
    fn raw_certificate_builds_fingerprint_identity() {
        let raw = RawCertificate::new(vec![0x30, 0x82]);

        let identity = raw
            .to_certificate_identity(SurfaceScope::Application)
            .unwrap();

        assert_eq!(identity.fingerprint().len(), 64);
        assert!(
            identity
                .fingerprint()
                .as_str()
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        );
        assert_eq!(
            identity.subject(),
            format!("sha256:{}", &identity.fingerprint().as_str()[..16])
        );
        assert_eq!(identity.surface_scope(), SurfaceScope::Application);
    }

    #[test]
    fn raw_certificate_identity_rejects_empty_der() {
        let error = RawCertificate::new(Vec::new())
            .to_certificate_identity(SurfaceScope::Application)
            .unwrap_err();

        assert_eq!(error.kind(), andromeda_error::AndromedaErrorKind::Security);
        assert!(error.message().contains("cannot be empty"));
    }

    #[test]
    fn parsed_certificate_rejects_empty_subject_cn() {
        let result = ParsedCertificate::new("  ".to_string(), "a".repeat(64), None);
        assert!(result.is_err());
    }

    #[test]
    fn parsed_certificate_rejects_invalid_fingerprint_length() {
        let result = ParsedCertificate::new(
            "test-cn".to_string(),
            "a".repeat(63), // Too short
            None,
        );
        assert!(result.is_err());

        let result = ParsedCertificate::new(
            "test-cn".to_string(),
            "a".repeat(65), // Too long
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn parsed_certificate_rejects_non_hex_fingerprint() {
        let result = ParsedCertificate::new(
            "test-cn".to_string(),
            "g".to_string() + &"a".repeat(63), // 'g' is not hex
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn parsed_certificate_accepts_valid_fingerprint() {
        let result = ParsedCertificate::new("test-cn".to_string(), "a".repeat(64), None);
        assert!(result.is_ok());
        let cert = result.unwrap();
        assert_eq!(cert.subject_cn, "test-cn");
        assert_eq!(cert.fingerprint_sha256.len(), 64);
    }

    #[test]
    fn validate_fingerprint_checks_length_and_encoding() {
        assert!(validate_fingerprint(&"a".repeat(64)).is_ok());
        assert!(validate_fingerprint(&"a".repeat(63)).is_err());
        assert!(validate_fingerprint(&("g".to_string() + &"a".repeat(63))).is_err());
    }

    #[test]
    fn to_certificate_identity_builds_observe_type() {
        let parsed =
            ParsedCertificate::new("test-service".to_string(), "a".repeat(64), None).unwrap();

        let identity = parsed
            .to_certificate_identity(SurfaceScope::Application)
            .unwrap();

        assert_eq!(identity.fingerprint().as_str(), "a".repeat(64));
        assert_eq!(identity.subject(), "test-service");
        assert_eq!(identity.surface_scope(), SurfaceScope::Application);
    }

    #[test]
    fn add_san_dns_name_deduplicates() {
        let parsed = ParsedCertificate::new("cn".to_string(), "a".repeat(64), None)
            .unwrap()
            .add_san_dns_name("dns1.example.com".to_string())
            .add_san_dns_name("dns1.example.com".to_string()); // duplicate

        assert_eq!(parsed.san_dns_names.len(), 1);
    }
}
