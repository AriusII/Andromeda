#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Types

Semantic identifiers and Procedure/ResultStream type descriptors shared across
Andromeda engine crates.
"#]

mod ids;
mod types;

pub use ids::{
    CatalogObjectId, CatalogVersion, ContractHash, DatabaseId, InvocationId, NamespaceId,
    ProcedureId, RequestId, SessionId, TransactionId,
};
pub use types::{
    AbsencePolicy, ColumnDescriptor, DecimalType, FloatMode, FloatType, ScalarType, TextEncoding,
    TextType, TimestampType, TypeDescriptor,
};
