use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub use andromeda_procedure_runtime::{
    PreTransactionDispatchEvidence, ProcedureDispatchRequest, ProcedureDispatchUnavailableReason,
    ProcedureRequestResolver,
};

pub type RemoteProcedureDispatcherUnavailable =
    andromeda_procedure_runtime::RemoteProcedureDispatcherUnavailable<crate::LocalProcedure>;
pub type SrplDispatcherAdapter = andromeda_procedure_runtime::SrplDispatcherAdapter<
    crate::SrplProcedureDispatcher,
    crate::LocalProcedure,
>;

pub trait ProcedureDispatcher:
    andromeda_procedure_runtime::ProcedureDispatcher<Procedure = crate::LocalProcedure>
{
    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<crate::LocalProcedure> {
        andromeda_procedure_runtime::ProcedureDispatcher::dispatch_procedure(self, request)
    }
}

impl<T> ProcedureDispatcher for T where
    T: andromeda_procedure_runtime::ProcedureDispatcher<Procedure = crate::LocalProcedure>
{
}

impl ProcedureRequestResolver for crate::SrplProcedureDispatcher {
    fn resolve_request(&self, request: &crate::InvocationRequest) -> AndromedaResult<()> {
        self.resolve_procedure(request)
            .map(|_| ())
            .map_err(|resolve_err| {
                AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    format!("SRPL procedure resolution failed: {:?}", resolve_err),
                )
            })
    }
}
