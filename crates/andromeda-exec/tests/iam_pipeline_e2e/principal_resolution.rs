use crate::support::{new_resolver, resolver_with_principal};
use andromeda_core::{AndromedaErrorKind, PrincipalRole};
use andromeda_exec::services::PrincipalResolver;

#[test]
fn test_principal_resolution_found() {
    let fingerprint = "test_resolution_found";
    let resolver = resolver_with_principal(fingerprint, PrincipalRole::User);

    let principal = resolver
        .resolve(fingerprint)
        .expect("registered principal should resolve");

    assert_eq!(principal.role, PrincipalRole::User);
    assert_eq!(principal.cert_fingerprint.as_str(), fingerprint);
}

#[test]
fn test_principal_resolution_not_found() {
    let resolver = new_resolver();

    let err = resolver
        .resolve("unknown_fingerprint")
        .expect_err("unknown fingerprint should be rejected");

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
}

#[test]
fn test_principal_resolution_empty_fingerprint() {
    let resolver = new_resolver();

    let result = resolver.resolve("");

    assert!(result.is_err());
}
