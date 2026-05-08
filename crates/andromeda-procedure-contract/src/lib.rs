#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Procedure Contract Model

Typed Procedure contracts, canonical contract hashes, policy versions, binding
evidence, and compatibility diagnostics.

This crate owns the Procedure contract surface and the minimal catalog identity
types required to bind contracts without depending on catalog storage. It does
not own catalog persistence, DefinitionBatch application, WAL codecs, RPC
runtime behavior, or StructuredObject payload serialization.
"#]

mod completion;
mod hash;
mod manifest;
mod materialization;
mod names;
mod type_encoding;
mod types;
mod validation;

pub use completion::*;
pub use manifest::*;
pub use materialization::*;
pub use names::*;
pub use types::*;
pub use validation::*;
