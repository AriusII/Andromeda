#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Resource

Typed resource budgets and limits with checked constructors.
"#]

mod admission;
mod error;
mod limits;
mod scope;

pub use admission::{
    ExecutionResourceAdmissionDecision, ExecutionResourceAdmissionRequest,
    ResourceAdmissionEvidence, ResourceAdmissionRejection,
};
pub use error::{
    ResourceLimitError, ResourceLimitField, ResourceResult, ResourceScopeError, ResourceScopeResult,
};
pub use limits::{
    ByteBudget, ByteLimit, ResourceBudget, ResourceLimits, StreamBudget, StreamLimit,
};
pub use scope::{ResourceBudgetScope, ResourceBudgetScopeKind, ResourceJobName};
