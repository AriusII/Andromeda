//! Runtime-free principal security vocabulary shared with the core compatibility surface.

mod contract;
mod permission;
mod surface_scope;

pub use permission::{ALL_PROCEDURES, Permission};
pub use surface_scope::SurfaceScope;
