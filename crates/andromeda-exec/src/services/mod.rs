pub mod admission;
pub mod completion;
pub mod pre_transaction;
pub mod result_validation;

// IAM services (Wave 19+)
pub mod permission_audit_emitter;
pub mod permission_evaluator;
pub mod principal_resolver;
pub mod transaction_error_routing;

pub use admission::*;
pub use completion::*;
pub use pre_transaction::*;
pub use result_validation::*;

// IAM re-exports
pub use permission_audit_emitter::{
    DenialAuditReason, NoOpPermissionAuditEmitter, PermissionAuditEmitter, PermissionAuditEvent,
    PermissionDecisionAudit,
};
pub use permission_evaluator::{
    ConcretePermissionEvaluator, DenialReason, PermissionDecision, PermissionEvaluator,
    PermissionEvaluatorImpl,
};
pub use principal_resolver::{LocalPrincipalResolver, PrincipalResolver};
pub use transaction_error_routing::{
    ErrorKind, RetryRouting, RoutedTransactionError, TerminalTxEvidence, TerminalTxJournal,
    TerminalTxState, route_transaction_error,
};
