use super::*;

pub(super) const VALID_SHA256: &str =
    "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

pub(super) fn seeded_sha256(seed: char) -> String {
    debug_assert!(seed.is_ascii_hexdigit());
    format!("{seed}1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2")
}

pub(super) fn fingerprint(value: impl Into<String>) -> CertificateFingerprint {
    CertificateFingerprint::new(value).expect("valid certificate fingerprint")
}

pub(super) fn user_principal() -> Principal {
    Principal::new(
        PrincipalId::new(1),
        PrincipalRole::User,
        SessionToken::new("test-token"),
        fingerprint("fingerprint"),
    )
    .expect("Principal creation should succeed")
}
