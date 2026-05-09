//! Compatibility facade for concrete SRPL execution adapter behavior.
//!
//! The implementation is owned by `andromeda-execution`; this module keeps the
//! historical `andromeda_exec` import path available while callers migrate.

pub use andromeda_execution::{
    FieldValue, SrplExecutionAdapter, SrplStreamBackpressure, SrplTransactionContext,
    SrplTypedEnvironment, StructuredObject,
};
