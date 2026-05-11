//! Security authorization boundaries for Andromeda runtime surfaces.

#![forbid(unsafe_code)]

mod break_glass_policy;
mod principal_binding;
mod surface_gate;

pub use break_glass_policy::BreakGlassPolicy;
pub use principal_binding::{
    AuthorizationDenialReason, AuthorizationOutcome, PrincipalBinding, PrincipalRegistry,
    SurfaceAction, SurfaceAuthorizer,
};
pub use surface_gate::{AuthorizedProcedureDispatch, SurfacePlaneAuthorizer};
