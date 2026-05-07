use super::*;
use crate::ProcedureId;
use helpers::{VALID_SHA256, seeded_sha256};

#[test]
fn test_from_certificate_fields_validates_cn() {
    let result = Principal::from_certificate_fields("mtls-service-001", VALID_SHA256);
    assert!(result.is_ok(), "valid CN must be accepted");

    let result = Principal::from_certificate_fields("", VALID_SHA256);
    assert!(result.is_err(), "empty CN must be rejected");

    let result = Principal::from_certificate_fields("   ", VALID_SHA256);
    assert!(result.is_err(), "whitespace-only CN must be rejected");
}

#[test]
fn test_from_certificate_fields_validates_fingerprint() {
    let valid_cn = "mtls-service-001";

    let result = Principal::from_certificate_fields(valid_cn, VALID_SHA256);
    assert!(result.is_ok(), "valid SHA-256 fingerprint must be accepted");

    let short_fp = "a1b2c3d4";
    let result = Principal::from_certificate_fields(valid_cn, short_fp);
    assert!(result.is_err(), "short fingerprint must be rejected");

    let non_hex_fp = "g1g2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6g1g2";
    let result = Principal::from_certificate_fields(valid_cn, non_hex_fp);
    assert!(result.is_err(), "non-hex fingerprint must be rejected");

    let result = Principal::from_certificate_fields(valid_cn, "");
    assert!(result.is_err(), "empty fingerprint must be rejected");
}

#[test]
fn test_from_certificate_fields_deterministic() {
    let cn = "mtls-svc-test";
    let fp = seeded_sha256('b');

    let p1 = Principal::from_certificate_fields(cn, &fp).expect("first creation");
    let p2 = Principal::from_certificate_fields(cn, &fp).expect("second creation");

    assert_eq!(p1.id, p2.id, "same fingerprint -> same principal ID");
    assert_eq!(
        p1.session_token, p2.session_token,
        "same fingerprint -> same session token"
    );
    assert_eq!(p1.cert_fingerprint, p2.cert_fingerprint);
    assert_eq!(p1.role, p2.role);
}

#[test]
fn test_from_certificate_fields_different_fingerprints_different_ids() {
    let cn = "mtls-svc-test";

    let p1 = Principal::from_certificate_fields(cn, VALID_SHA256).expect("first principal");
    let fp2 = seeded_sha256('b');
    let p2 = Principal::from_certificate_fields(cn, &fp2).expect("second principal");

    assert_ne!(
        p1.id, p2.id,
        "different fingerprints must produce different principal IDs"
    );
    assert_ne!(
        p1.session_token, p2.session_token,
        "different fingerprints must produce different session tokens"
    );
}

#[test]
fn test_from_certificate_fields_non_zero_id() {
    let principal = Principal::from_certificate_fields("mtls-svc-test", VALID_SHA256)
        .expect("principal created");

    assert!(
        !principal.id.is_zero(),
        "certificate-derived principal ID must be non-zero"
    );
}

#[test]
fn test_from_certificate_fields_produces_user_role() {
    let fp = seeded_sha256('c');

    let principal =
        Principal::from_certificate_fields("mtls-svc-test", &fp).expect("principal created");

    assert_eq!(
        principal.role,
        PrincipalRole::User,
        "certificate-derived principal must have User role by default"
    );
}

#[test]
fn test_from_certificate_fields_session_token_format() {
    let fp = seeded_sha256('d');

    let principal =
        Principal::from_certificate_fields("mtls-svc-test", &fp).expect("principal created");

    assert!(
        principal.session_token.as_str().starts_with("mtls:"),
        "session token must be prefixed with 'mtls:'"
    );
    assert!(
        !principal.session_token.is_empty(),
        "session token must not be empty"
    );
}

#[test]
fn test_from_certificate_fields_permission_evaluation() {
    let fp = seeded_sha256('e');

    let principal =
        Principal::from_certificate_fields("mtls-svc-test", &fp).expect("principal created");

    assert!(
        principal.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(u64::MAX))),
        "user role must execute procedures"
    );
    assert!(
        principal.has_permission(&Permission::ReadContractMetadata),
        "user role must read contracts"
    );
    assert!(
        !principal.has_permission(&Permission::AdminShutdown),
        "user role must not shutdown"
    );
}
