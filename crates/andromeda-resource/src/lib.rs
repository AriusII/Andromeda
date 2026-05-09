#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Resource

Typed resource budgets and limits with checked constructors.
"#]

mod admission;
mod error;
mod limits;

pub use admission::{
    ExecutionResourceAdmissionDecision, ExecutionResourceAdmissionRequest,
    ResourceAdmissionEvidence, ResourceAdmissionRejection,
};
pub use error::{ResourceLimitError, ResourceLimitField, ResourceResult};
pub use limits::{
    ByteBudget, ByteLimit, ResourceBudget, ResourceLimits, StreamBudget, StreamLimit,
};
