//! V0 principal binding and surface authorization contract.
//!
//! [`PrincipalBinding`] anchors a certificate fingerprint to one
//! [`UserPrincipal`] and a normalized permission set. [`SurfaceAuthorizer`]
//! evaluates that binding against the requested [`SurfaceScope`] and always
//! returns a [`SecurityAuditTrace`] for both allow and deny decisions.

mod authorizer;
mod bridge;
mod model;
mod registry;

pub use crate::events::SecurityAuditDenialReason as AuthorizationDenialReason;
pub use authorizer::SurfaceAuthorizer;
pub use bridge::{core_principal_to_observe_user_principal, observe_user_principal_to_core};
pub use model::{AuthorizationOutcome, PrincipalBinding, SurfaceAction};
pub use registry::PrincipalRegistry;

#[cfg(test)]
mod tests;
