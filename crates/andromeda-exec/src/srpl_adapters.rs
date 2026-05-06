//! SRPL execution adapter boundary for typed IR operations.

mod adapter;
mod backpressure;
mod environment;
mod transaction_context;
mod values;

pub use adapter::SrplExecutionAdapter;
pub use backpressure::SrplStreamBackpressure;
pub use environment::SrplTypedEnvironment;
pub use transaction_context::SrplTransactionContext;
pub use values::{FieldValue, StructuredObject};

#[cfg(test)]
mod tests;
