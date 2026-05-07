use super::*;
use helpers::seeded_sha256;

#[test]
fn test_session_token_creation() {
    let token = SessionToken::new("test-token-123");
    assert_eq!(token.as_str(), "test-token-123");
    assert!(!token.is_empty());
}

#[test]
fn test_session_token_empty() {
    let token = SessionToken::new("");
    assert!(token.is_empty());
}

#[test]
fn test_session_token_display() {
    let token = SessionToken::new("my-session");
    assert_eq!(token.to_string(), "my-session");
}

#[test]
fn test_certificate_derived_session_token_is_truncated_and_prefixed() {
    let fp = CertificateFingerprint::new(seeded_sha256('a')).unwrap();

    let token = SessionToken::from_certificate_fingerprint(&fp);

    assert_eq!(token.as_str(), "mtls:a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4");
    assert!(
        !token.as_str().contains(&fp.as_str()[32..]),
        "derived token must not embed the full fingerprint"
    );
}

#[test]
fn test_certificate_derived_session_token_is_non_secret_evidence() {
    let fp = CertificateFingerprint::new(seeded_sha256('f')).unwrap();

    let token = SessionToken::from_certificate_fingerprint(&fp);

    assert_eq!(
        token.to_string(),
        token.as_str(),
        "session token display is intentionally unmasked audit evidence, not a bearer secret"
    );
}

#[test]
fn test_certificate_derived_session_token_from_zero_fingerprint_is_valid_evidence() {
    let fp = CertificateFingerprint::new(
        "0000000000000000000000000000000000000000000000000000000000000000",
    )
    .unwrap();

    let token = SessionToken::from_certificate_fingerprint(&fp);

    assert_eq!(token.as_str(), "mtls:00000000000000000000000000000000");
    assert!(!token.is_empty());
}
