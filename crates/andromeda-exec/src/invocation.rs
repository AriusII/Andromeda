use andromeda_core::AndromedaResult;

use crate::InvocationCompletion;

pub use andromeda_admission::{InvocationReject, InvocationRequest};

pub trait ProcedureInvoker {
    fn invoke(&self, request: InvocationRequest) -> AndromedaResult<InvocationCompletion>;
}
