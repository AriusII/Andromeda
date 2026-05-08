#![forbid(unsafe_code)]
#![doc = r#"
# andromeda-admission

Owner for pre-transaction Procedure admission.

This crate proves admission before transaction, typed Procedure contract
binding, no transaction on denial, no business hardcoding, and no
application-facing SQL.
"#]

mod context;
mod invocation;
mod io_admission;
mod permission_evaluator;
mod pre_transaction;
mod service;

pub use andromeda_result_stream::CompletionStatus;
pub use context::InvocationContext;
pub use invocation::{InvocationReject, InvocationRequest};
pub use io_admission::{ExecutionIoAdmissionDecision, ExecutionIoAdmissionRequest};
pub use permission_evaluator::{DenialReason, PermissionDecision, PermissionEvaluator};
pub use pre_transaction::{PreTransactionValidationService, ProcedureBindingEvidence};
pub use service::AdmissionService;
