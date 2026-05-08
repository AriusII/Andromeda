use std::sync::Arc;

use super::{ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse};

/// Pre-transaction resolver for SRPL executable plans and manifests.
pub trait ProcedureResolver: Send + Sync {
    fn resolve_procedure(
        &self,
        request: ProcedureResolveRequest,
    ) -> Result<ProcedureResolveResponse, ProcedureResolveError>;
}

impl<T> ProcedureResolver for Arc<T>
where
    T: ProcedureResolver + ?Sized,
{
    fn resolve_procedure(
        &self,
        request: ProcedureResolveRequest,
    ) -> Result<ProcedureResolveResponse, ProcedureResolveError> {
        (**self).resolve_procedure(request)
    }
}
