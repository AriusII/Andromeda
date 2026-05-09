//! Compatibility reexports for the V0 principal binding and surface authorization contract.
//!
//! Authority for these types now lives in `andromeda-security`. Observe keeps
//! this module so existing envelope/query callers can migrate imports without
//! coupling the security crate back to observe.

pub use andromeda_security::{
    AuthorizationDenialReason, AuthorizationOutcome, PrincipalBinding, PrincipalRegistry,
    SurfaceAction, SurfaceAuthorizer,
};

#[cfg(test)]
mod tests;
