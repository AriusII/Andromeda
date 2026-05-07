use andromeda_core::{
    AndromedaResult, CatalogVersion, ContractHash, InvocationId, ProcedureId, RequestId, SessionId,
    TransactionId,
};
use andromeda_observe::CertificateIdentity;

use crate::{
    CatalogProcedureManifest, Connection, DispatchPolicy, FrameBytes, FrameType,
    ResultStreamMetadataPolicy, StreamRole, SurfacePlane, TransportSurface,
    TypedResultStreamBounds, TypedResultStreamContext, validate_transport_surface,
};

use super::errors::{protocol_error, security_error};
use super::{state, validation};

/// Domain projection of the protobuf `RpcExecuteRequest` admitted by the
/// Procedure gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureRouteExecuteRequest {
    pub procedure_name: String,
    pub expected_contract_hash: ContractHash,
    pub expected_catalog_version: CatalogVersion,
    pub expected_stats_version: u64,
    pub surface_scope: String,
    pub argument_count: usize,
}

/// Pre-dispatch route evidence for an Application-surface Procedure invocation.
///
/// The execution layer can consume this binding knowing the transport surface,
/// mTLS identity, frame context, requested contract identity, and resolved
/// catalog manifest were validated before transaction creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureRouteBinding {
    pub invocation_id: InvocationId,
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub tx_id: Option<TransactionId>,
    pub surface_plane: SurfacePlane,
    pub certificate_identity: CertificateIdentity,
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub stats_version: u64,
    pub execute_request: ProcedureRouteExecuteRequest,
    pub manifest: CatalogProcedureManifest,
}

pub(super) struct ProcedureRouteAdmission<'a> {
    pub(super) connection: &'a Connection,
    pub(super) certificate_identity: &'a CertificateIdentity,
    pub(super) plane: SurfacePlane,
}

pub(super) fn bind_application_procedure_route(
    admission: ProcedureRouteAdmission<'_>,
    stream_id: u64,
    frame: &FrameBytes,
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<ProcedureRouteBinding> {
    state::validate_dispatch_preconditions(admission.connection)?;

    if admission.plane != SurfacePlane::Application {
        return Err(security_error(
            "procedure invocation route requires Application surface",
        ));
    }
    state::validate_frame_session_binding(admission.connection, frame)?;

    validate_transport_surface(
        frame,
        TransportSurface::ReliableStream(StreamRole::CommandBidirectional),
    )?;
    if frame.header.frame_type != FrameType::RpcExecuteRequest {
        return Err(protocol_error(
            "procedure invocation route requires RpcExecuteRequest frame",
        ));
    }
    if frame.header.tx_id.is_some() {
        return Err(protocol_error(
            "procedure invocation request must not carry client transaction id",
        ));
    }

    let execute_request =
        validation::decode_and_validate_rpc_execute_request(frame, admission.plane, manifest)?;

    Ok(ProcedureRouteBinding {
        invocation_id: state::invocation_id_for_stream(stream_id),
        request_id: frame.header.request_id,
        session_id: frame.header.session_id,
        tx_id: frame.header.tx_id,
        surface_plane: admission.plane,
        certificate_identity: admission.certificate_identity.clone(),
        procedure_id: manifest.procedure_id,
        contract_hash: manifest.contract_hash,
        catalog_version: manifest.catalog_version,
        stats_version: manifest.stats_version,
        execute_request,
        manifest: manifest.clone(),
    })
}

impl ProcedureRouteBinding {
    /// Returns the exact typed ResultStream context admitted for this Procedure
    /// invocation.
    ///
    /// Result emitters can pass this value to
    /// `validate_typed_result_stream_sequence_with_context_and_bounds` so
    /// metadata, batches, and completion stay bound to the same request,
    /// session, transaction, ContractHash, and CatalogVersion accepted before
    /// executor dispatch.
    pub const fn typed_result_stream_context(&self) -> TypedResultStreamContext {
        TypedResultStreamContext::new(
            self.request_id,
            self.session_id,
            self.tx_id,
            self.contract_hash,
            self.catalog_version,
        )
    }

    /// Creates the only dispatch policy shape allowed to emit ResultStream
    /// frames for this admitted Procedure invocation.
    pub const fn result_stream_dispatch_policy(
        &self,
        metadata_policy: ResultStreamMetadataPolicy,
    ) -> DispatchPolicy {
        DispatchPolicy::new_result_stream_with_metadata_policy(
            self.typed_result_stream_context(),
            metadata_policy,
        )
    }

    /// Creates a bounded ResultStream dispatch policy for this admitted
    /// Procedure invocation.
    pub const fn bounded_result_stream_dispatch_policy(
        &self,
        metadata_policy: ResultStreamMetadataPolicy,
        bounds: TypedResultStreamBounds,
    ) -> DispatchPolicy {
        DispatchPolicy::new_result_stream_with_context_and_bounds(
            self.typed_result_stream_context(),
            metadata_policy,
            bounds,
        )
    }
}
