use super::*;
use helpers::seeded_sha256;

#[test]
fn test_certificate_fingerprint_creation() {
    let fp = CertificateFingerprint::new("a1b2c3d4").unwrap();
    assert_eq!(fp.as_str(), "a1b2c3d4");
}

#[test]
fn test_certificate_fingerprint_empty_rejected() {
    let fp = CertificateFingerprint::new("");
    assert!(fp.is_none());
}

#[test]
fn test_certificate_fingerprint_whitespace_only_rejected() {
    let fp = CertificateFingerprint::new("   ");
    assert!(fp.is_none());
}

#[test]
fn test_certificate_fingerprint_creation_trims_edges() {
    let fp = CertificateFingerprint::new("  a1b2c3d4  ").unwrap();
    assert_eq!(fp.as_str(), "a1b2c3d4");
}

#[test]
fn test_certificate_fingerprint_valid_sha256() {
    let fp = CertificateFingerprint::new(seeded_sha256('a')).unwrap();
    assert!(fp.is_valid_sha256());
}

#[test]
fn test_certificate_fingerprint_invalid_sha256_too_short() {
    let short = "a1b2c3d4e5f6";
    let fp = CertificateFingerprint::new(short).unwrap();
    assert!(!fp.is_valid_sha256());
}

#[test]
fn test_certificate_fingerprint_invalid_sha256_non_hex() {
    let non_hex = "g1g2g3g4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6g1g2";
    let fp = CertificateFingerprint::new(non_hex).unwrap();
    assert!(!fp.is_valid_sha256());
}

#[test]
fn test_certificate_fingerprint_len() {
    let fp = CertificateFingerprint::new("test").unwrap();
    assert_eq!(fp.len(), 4);
}

#[test]
fn test_certificate_fingerprint_display() {
    let fp = CertificateFingerprint::new("abc123").unwrap();
    assert_eq!(fp.to_string(), "abc123");
}

#[test]
fn test_certificate_fingerprint_new_accepts_owned_string() {
    let fp = CertificateFingerprint::new("test".to_string()).unwrap();
    assert_eq!(fp.as_str(), "test");
}

#[test]
fn test_certificate_fingerprint_new_accepts_borrowed_str() {
    let fp = CertificateFingerprint::new("test").unwrap();
    assert_eq!(fp.as_str(), "test");
}

#[test]
fn test_certificate_fingerprint_unchecked() {
    let fp = CertificateFingerprint::new_unchecked("");
    assert!(fp.is_empty());
}

#[test]
fn test_certificate_identity_requires_sha256_subject_and_surface() {
    let valid =
        CertificateIdentity::new(seeded_sha256('2'), "CN=svc-app", SurfaceScope::Application)
            .expect("valid certificate identity");

    assert_eq!(valid.surface_scope(), SurfaceScope::Application);
    assert_eq!(valid.status(), CertificateIdentityStatus::Active);
    assert!(valid.has_identity_evidence());

    let bad_fingerprint =
        CertificateIdentity::new("short", "CN=svc-app", SurfaceScope::Application);
    assert!(bad_fingerprint.is_err());

    let bad_subject =
        CertificateIdentity::new(seeded_sha256('3'), "   ", SurfaceScope::Application);
    assert!(bad_subject.is_err());
}
