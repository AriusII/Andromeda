use crate::support::new_resolver;
use andromeda_iam::PrincipalResolver;
use andromeda_principal::PrincipalRole;

#[test]
fn test_principal_session_token_uniqueness() {
    let resolver = new_resolver();

    resolver
        .register_principal("user1".to_string(), PrincipalRole::User)
        .expect("registration should succeed");
    resolver
        .register_principal("user2".to_string(), PrincipalRole::User)
        .expect("registration should succeed");

    let principal1 = resolver
        .resolve("user1")
        .expect("first user should resolve");
    let principal2 = resolver
        .resolve("user2")
        .expect("second user should resolve");

    assert_ne!(principal1.session_token, principal2.session_token);
}

#[test]
fn test_resolver_list_principals() {
    let resolver = new_resolver();

    for i in 0..3 {
        let fingerprint = format!("list_test_{}", i);
        resolver
            .register_principal(fingerprint, PrincipalRole::User)
            .expect("registration should succeed");
    }

    let principals = resolver
        .list_principals()
        .expect("principal list should be readable");

    assert_eq!(principals.len(), 3);
    for principal in principals {
        assert_eq!(principal.role, PrincipalRole::User);
    }
}

#[test]
fn test_resolver_fingerprints_list() {
    let resolver = new_resolver();

    let fingerprints_in = vec!["fp_1", "fp_2", "fp_3"];
    for fp in &fingerprints_in {
        resolver
            .register_principal((*fp).to_string(), PrincipalRole::User)
            .expect("registration should succeed");
    }

    let fingerprints_out = resolver.fingerprints();

    assert_eq!(fingerprints_out.len(), 3);
    for fp in fingerprints_out {
        assert!(fingerprints_in.contains(&fp.as_str()));
    }
}
