use super::{ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse};

/// Pre-transaction resolver for SRPL executable plans and manifests.
pub trait ProcedureResolver: Send + Sync {
    fn resolve_procedure(
        &self,
        request: ProcedureResolveRequest,
    ) -> Result<ProcedureResolveResponse, ProcedureResolveError>;
}
