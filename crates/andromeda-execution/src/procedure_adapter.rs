use andromeda_error::AndromedaResult;

use crate::ProcedureDispatchRequest;

/// Generic execution-side adapter boundary that dispatches an admitted
/// Procedure request into a typed local procedure representation.
pub trait ExecutionProcedureDispatcher {
    type Procedure;

    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<Self::Procedure>;
}

pub type RemoteProcedureDispatcherUnavailable<Procedure> =
    andromeda_procedure_runtime::RemoteProcedureDispatcherUnavailable<Procedure>;
pub type SrplProcedureRuntimeAdapter<Resolver, Procedure> =
    andromeda_procedure_runtime::SrplDispatcherAdapter<Resolver, Procedure>;

impl<T> ExecutionProcedureDispatcher for T
where
    T: andromeda_procedure_runtime::ProcedureDispatcher,
{
    type Procedure = <T as andromeda_procedure_runtime::ProcedureDispatcher>::Procedure;

    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<Self::Procedure> {
        andromeda_procedure_runtime::ProcedureDispatcher::dispatch_procedure(self, request)
    }
}
