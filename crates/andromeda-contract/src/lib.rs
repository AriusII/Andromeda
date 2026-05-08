#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Contract Model

Canonical contract and catalog-object descriptor facade shared by catalog,
SRPL, execution, and protocol-facing crates.

This crate owns catalog object descriptors and reexports the split Procedure
contract and StructuredObject contract crates. It does not own catalog storage,
DefinitionBatch application, WAL codecs, RPC runtime behavior, or
StructuredObject wire payloads.
"#]

mod dependencies;
mod objects;

pub use andromeda_procedure_contract::*;
pub use andromeda_structured_object::*;
pub use dependencies::*;
pub use objects::*;
