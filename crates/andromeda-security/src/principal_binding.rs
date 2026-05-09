//! Principal binding and surface authorization authority.
//!
//! This module owns the V0 certificate-to-principal binding registry used by
//! dispatch-time security gates. Audit and observe crates consume the emitted
//! [`SecurityAuditTrace`] payloads; they do not own the authorization state.

mod authorizer;
mod model;
mod registry;

pub use andromeda_audit::SecurityAuditDenialReason as AuthorizationDenialReason;
pub use authorizer::SurfaceAuthorizer;
pub use model::{AuthorizationOutcome, PrincipalBinding, SurfaceAction};
pub use registry::PrincipalRegistry;

#[cfg(test)]
mod tests;
