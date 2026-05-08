//! Security authorization boundaries for Andromeda runtime surfaces.

#![forbid(unsafe_code)]

mod surface_gate;

pub use surface_gate::{AuthorizedProcedureDispatch, SurfacePlaneAuthorizer};
