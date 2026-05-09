use super::*;
#[test]
fn test_principal_id_creation() {
    let id = PrincipalId::new(42);
    assert_eq!(id.get(), 42);
    assert!(!id.is_zero());
}

#[test]
fn test_principal_id_zero() {
    let id = PrincipalId::new(0);
    assert!(id.is_zero());
}

#[test]
fn test_principal_id_display() {
    let id = PrincipalId::new(42);
    assert_eq!(id.to_string(), "PrincipalId(42)");
}

#[test]
fn test_principal_id_from_u64() {
    let id = PrincipalId::from(123u64);
    assert_eq!(id.get(), 123);
}

#[test]
fn test_principal_id_from_sha256_fingerprint_uses_core_rule() {
    let fp = CertificateFingerprint::new(
        "00000000e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
    )
    .unwrap();

    let id = PrincipalId::from_certificate_fingerprint(&fp).unwrap();

    assert_eq!(id.get(), 1, "zero-derived ids must be remapped to one");
}

#[test]
fn test_principal_id_from_freeform_fingerprint_preserves_legacy_dev_derivation() {
    let fp = CertificateFingerprint::new("test_user_fingerprint").unwrap();

    let id = PrincipalId::from_certificate_fingerprint(&fp).unwrap();
    let expected = fp.as_str().as_bytes().iter().fold(0u64, |acc, &byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u64)
    });

    assert_eq!(id.get(), expected);
}
