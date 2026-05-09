//! Security authorization boundaries for Andromeda runtime surfaces.

#![forbid(unsafe_code)]

mod principal_binding;
mod surface_gate;

pub use principal_binding::{
    AuthorizationDenialReason, AuthorizationOutcome, PrincipalBinding, PrincipalRegistry,
    SurfaceAction, SurfaceAuthorizer,
};
pub use surface_gate::{AuthorizedProcedureDispatch, SurfacePlaneAuthorizer};
