#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Contract Model

Canonical contract and catalog-object descriptors shared by catalog, SRPL,
execution, and protocol-facing crates.

This crate owns contract identities and shape descriptors only. It does not own
catalog storage, DefinitionBatch application, WAL codecs, RPC runtime behavior,
or StructuredObject wire payloads.
"#]

mod contracts;
mod dependencies;
mod names;
mod objects;

pub use contracts::*;
pub use dependencies::*;
pub use names::*;
pub use objects::*;
