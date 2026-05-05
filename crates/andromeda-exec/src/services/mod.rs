pub mod admission;
pub mod completion;
pub mod pre_transaction;
pub mod result_validation;

// IAM services (Wave 19+)
pub mod principal_resolver;
pub mod permission_evaluator;

pub use admission::*;
pub use completion::*;
pub use pre_transaction::*;
pub use result_validation::*;

// IAM re-exports
pub use principal_resolver::{LocalPrincipalResolver, PrincipalResolver};
pub use permission_evaluator::{
    ConcretePermissionEvaluator, DenialReason, PermissionDecision, PermissionEvaluator,
    PermissionEvaluatorImpl,
};
