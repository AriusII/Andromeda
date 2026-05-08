#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Policy

Runtime-free policy identity, version, and admission stance primitives.
"#]

mod admission;
mod identity;

pub use admission::AdmissionStance;
pub use identity::{PolicyId, PolicyVersion};
