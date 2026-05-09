//! Principal binding and surface authorization authority.
//!
//! This module owns the V0 certificate-to-principal binding registry used by
//! dispatch-time security gates. Audit and observe crates consume the emitted
//! [`SecurityAuditTrace`] payloads; they do not own the authorization state.

mod authorizer;
mod model;
mod registry;

pub use andromeda_security_contract::AuthorizationDenialReason;
pub use authorizer::SurfaceAuthorizer;
pub use model::{AuthorizationOutcome, PrincipalBinding, SurfaceAction};
pub(crate) use model::{allowed_security_outcome, denied_security_outcome};
pub use registry::PrincipalRegistry;

#[cfg(test)]
mod tests;
