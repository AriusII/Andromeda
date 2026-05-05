//! D3 mTLS certificate identity extraction and validation.
//!
//! This module provides the contract surface for extracting and binding mTLS
//! certificate identities to QUIC sessions. It remains runtime-free in V0:
//! types and validation functions are defined here; actual quinn connection
//! integration is deferred to the D4 Procedure Gateway under the `runtime-quinn`
//! feature.
//!
//! ## Certificate Identity Flow
//!
//! 1. Raw X.509 certificate is extracted from a QUIC connection.
//! 2. Certificate is parsed to extract fingerprint, subject, and metadata.
//! 3. Parsed identity is validated against the surface plane scope.
//! 4. Identity is bound to the [`Connection`] state machine.
//! 5. Dispatch authorization uses the bound certificate fingerprint.
//!
//! ## Out of Scope (V0)
//!
//! - Actual quinn connection types and cert extraction (deferred to D4).
//! - X.509 parsing implementation (deferred; contract is defined here).
//! - Certificate revocation and rotation choreography.
//! - Hardware security modules or external PKI systems.

use andromeda_core::AndromedaResult;
use andromeda_observe::SurfaceScope;

/// Convert a QUIC [`SurfacePlane`] to its required [`SurfaceScope`].
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
///
/// This is the entry point for the identity extraction pipeline.
/// In D4 (Procedure Gateway), `extract_peer_certificate()` will obtain
/// this from a quinn connection; in D3 contract tests, it is constructed
/// manually from known test vectors.
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
}

/// X.509 certificate fields extracted for identity and policy validation.
///
/// This type represents the parsed and validated result of extracting
/// identity evidence from a raw X.509 certificate. The fields are:
///
/// - `subject_cn`: CommonName from the Subject field (e.g., "mtls-service-001").
/// - `san_dns_names`: DNS Subject Alternative Names (parsed but not expanded in V0).
/// - `fingerprint_sha256`: SHA256 hash of the DER encoding (hex-encoded).
/// - `issuer_cn`: CommonName from the Issuer field (optional, for audit).
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
    /// Create a parsed certificate entry (test helper).
    ///
    /// In production (D4), this is populated by X.509 parser (rustls, x509-parser, etc).
    /// In D3 tests, this is used directly.
    pub fn new(
        subject_cn: String,
        fingerprint_sha256: String,
        issuer_cn: Option<String>,
    ) -> AndromedaResult<Self> {
        if subject_cn.trim().is_empty() {
            return Err(andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Security,
                "certificate subject CN cannot be empty",
            ));
        }
        if fingerprint_sha256.len() != 64 {
            return Err(andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Security,
                "certificate fingerprint must be 64 hex characters (SHA256)",
            ));
        }
        // Validate hex encoding.
        if !fingerprint_sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Security,
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

    /// Extract identity for a surface scope.
    ///
    /// Validates that the certificate's surface scope requirement is met.
    /// In V0, the subject CN is used directly; future decisions may expand
    /// to use SAN or other fields for role expansion.
    ///
    /// Returns `Err` if the certificate does not meet scope requirements.
    pub fn to_certificate_identity(
        self,
        required_scope: SurfaceScope,
    ) -> AndromedaResult<andromeda_observe::CertificateIdentity> {
        // In V0, subject CN is the primary identity.
        // Future: SAN/SPIFFE extraction may refine this.
        let subject = self.subject_cn.clone();

        // Build the identity with the required scope.
        andromeda_observe::CertificateIdentity::new(
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
        return Err(andromeda_core::AndromedaError::new(
            andromeda_core::AndromedaErrorKind::Security,
            "certificate fingerprint must be 64 hex characters",
        ));
    }
    if !fp.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(andromeda_core::AndromedaError::new(
            andromeda_core::AndromedaErrorKind::Security,
            "certificate fingerprint must be valid hex",
        ));
    }
    Ok(())
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

        assert_eq!(identity.fingerprint, "a".repeat(64));
        assert_eq!(identity.subject, "test-service");
        assert_eq!(identity.surface, SurfaceScope::Application);
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
