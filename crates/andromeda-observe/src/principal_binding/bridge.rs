use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::events::UserPrincipal;

/// Map observe::UserPrincipal -> core::Principal
///
/// Converts the audit-traced UserPrincipal (String ID, Kind enum) to the internal
/// core Principal (u64 ID, Role enum). Used during authorization enforcement to bridge
/// observability data to permission evaluation.
///
/// # Arguments
/// - `ouser`: The observe UserPrincipal from an audit trace
/// - `fingerprint`: The certificate fingerprint bound to this principal
/// - `role`: The core Principal role (derived from context or registry)
///
/// # Returns
/// - `Ok(Principal)` if mapping succeeds
/// - `Err` if observe principal_id is not parseable as u64 or principal creation fails
///
/// # Invariants
/// - Principal ID must be non-zero
/// - Session token is derived from the bound certificate fingerprint via core
/// - All constituent fields must pass Principal validation
pub fn observe_user_principal_to_core(
    ouser: &UserPrincipal,
    fingerprint: &andromeda_core::CertificateFingerprint,
    role: andromeda_core::PrincipalRole,
) -> AndromedaResult<andromeda_core::Principal> {
    let id_val: u64 = ouser.principal_id.parse().map_err(|_| {
        AndromedaError::new(
            AndromedaErrorKind::Security,
            format!(
                "observe principal_id '{}' must be parseable as u64",
                ouser.principal_id
            ),
        )
    })?;

    let principal_id = andromeda_core::PrincipalId::new(id_val);
    if principal_id.is_zero() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Security,
            "principal id must be non-zero",
        ));
    }

    let session_token = andromeda_core::SessionToken::from_certificate_fingerprint(fingerprint);

    andromeda_core::Principal::new(principal_id, role, session_token, fingerprint.clone())
        .ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Security,
                "core principal creation from observe type failed: invariant violation",
            )
        })
}

/// Map core::Principal -> observe::UserPrincipal
///
/// Converts the internal core Principal (u64 ID, Role enum) to audit-traced
/// UserPrincipal (String ID, Kind enum). Used for logging and emitting SecurityAuditTrace
/// events during authorization decisions.
///
/// # Arguments
/// - `cp`: The core Principal to convert
/// - `kind`: The UserPrincipalKind (Human, Service, BreakGlass)
///
/// # Returns
/// - `Ok(UserPrincipal)` with principal ID as decimal string
/// - `Err` if observe UserPrincipal creation fails (invalid evidence)
///
/// # Invariants
/// - Principal ID is converted to decimal string (non-empty)
/// - Kind is preserved from argument
pub fn core_principal_to_observe_user_principal(
    cp: &andromeda_core::Principal,
    kind: crate::events::UserPrincipalKind,
) -> AndromedaResult<UserPrincipal> {
    let principal_id = format!("{}", cp.id.get());
    UserPrincipal::new(principal_id, kind)
}
