use andromeda_error::AndromedaResult;
use andromeda_principal::{CertificateIdentity, SurfaceScope};
use andromeda_rpc_codec::{
    CatalogManifestResolutionRequest, CatalogManifestResolutionResponse,
    decode_catalog_manifest_resolution_route_request,
    encode_catalog_manifest_resolution_response_frame,
    validate_catalog_manifest_resolution_response_context,
};
use andromeda_types::TransactionId;
use andromeda_types::{RequestId, SessionId};

use andromeda_rpc::{TransportSurface, validate_transport_surface};
use andromeda_rpc_protocol::{FrameBytes, FrameType, StreamRole};

use crate::{SurfacePlane, TransportEndpointMetadata, TransportMessage};

use super::errors::{protocol_error, security_error};

/// Runtime context attached to one catalog manifest resolution route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolutionContext {
    request_id: RequestId,
    session_id: SessionId,
    tx_id: Option<TransactionId>,
    surface_plane: SurfacePlane,
    certificate_identity: Option<CertificateIdentity>,
    trace_id: Option<String>,
}

impl CatalogManifestResolutionContext {
    pub(super) fn from_route(
        request_id: RequestId,
        session_id: SessionId,
        tx_id: Option<TransactionId>,
        surface_plane: SurfacePlane,
        certificate_identity: Option<CertificateIdentity>,
        trace_id: Option<String>,
    ) -> Self {
        Self {
            request_id,
            session_id,
            tx_id,
            surface_plane,
            certificate_identity,
            trace_id,
        }
    }

    /// Request id shared by the frame header, protobuf envelope, and catalog request.
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }

    /// Session id shared by the frame header and protobuf envelope.
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Transaction id attached to the command stream frame, if present.
    pub const fn tx_id(&self) -> Option<TransactionId> {
        self.tx_id
    }

    /// Surface plane on which the request was admitted.
    pub const fn surface_plane(&self) -> SurfacePlane {
        self.surface_plane
    }

    /// Bound mTLS certificate identity, if the transport supplied one.
    pub fn certificate_identity(&self) -> Option<&CertificateIdentity> {
        self.certificate_identity.as_ref()
    }

    /// Trace id copied from the catalog request, if present.
    pub fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }
}

/// Local runtime trait for resolving catalog procedure manifests.
///
/// This keeps QUIC independent from the concrete catalog runtime while still
/// binding the typed protobuf request/response contract at the route boundary.
pub trait CatalogManifestResolutionRuntime {
    fn resolve_catalog_manifest(
        &mut self,
        context: &CatalogManifestResolutionContext,
        request: CatalogManifestResolutionRequest,
    ) -> AndromedaResult<CatalogManifestResolutionResponse>;
}

/// Reliable-stream gateway for catalog manifest resolution frames.
#[derive(Debug)]
pub struct CatalogManifestResolutionGateway<R> {
    runtime: R,
}

impl<R> CatalogManifestResolutionGateway<R> {
    pub const fn new(runtime: R) -> Self {
        Self { runtime }
    }

    pub const fn runtime(&self) -> &R {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut R {
        &mut self.runtime
    }

    pub fn into_runtime(self) -> R {
        self.runtime
    }
}

impl<R: CatalogManifestResolutionRuntime> CatalogManifestResolutionGateway<R> {
    /// Routes a validated transport message through the catalog manifest runtime.
    pub fn route_transport_message(
        &mut self,
        request: TransportMessage,
    ) -> AndromedaResult<TransportMessage> {
        let metadata = request.metadata().clone();
        let response_frame = self.route_frame(request.frame(), &metadata, request.stream_role())?;

        TransportMessage::new(metadata, StreamRole::CommandBidirectional, response_frame)
    }

    /// Routes one `ContractRequest` frame into one `ContractResponse` frame.
    pub fn route_frame(
        &mut self,
        frame: &FrameBytes,
        metadata: &TransportEndpointMetadata,
        stream_role: StreamRole,
    ) -> AndromedaResult<FrameBytes> {
        let admission = admit_catalog_manifest_route(frame, metadata, stream_role)?;

        let decoded_request = decode_catalog_manifest_resolution_route_request(frame)?;
        let request_envelope = decoded_request.envelope;
        let request = decoded_request.request;

        let context = CatalogManifestResolutionContext::from_route(
            frame.header.request_id,
            frame.header.session_id,
            frame.header.tx_id,
            admission.surface_plane,
            admission.certificate_identity,
            request.trace_id.clone(),
        );

        let request_for_validation = request.clone();
        let response = self.runtime.resolve_catalog_manifest(&context, request)?;
        validate_catalog_manifest_resolution_response_context(context.request_id(), &response)?;
        response.validate_against_request(&request_for_validation)?;

        let response_frame = encode_catalog_manifest_resolution_response_frame(
            &request_envelope,
            frame.header,
            &response,
        )?;
        validate_transport_surface(
            &response_frame,
            TransportSurface::ReliableStream(StreamRole::CommandBidirectional),
        )?;

        Ok(response_frame)
    }
}

struct CatalogManifestRouteAdmission {
    surface_plane: SurfacePlane,
    certificate_identity: Option<CertificateIdentity>,
}

fn admit_catalog_manifest_route(
    frame: &FrameBytes,
    metadata: &TransportEndpointMetadata,
    stream_role: StreamRole,
) -> AndromedaResult<CatalogManifestRouteAdmission> {
    if stream_role != StreamRole::CommandBidirectional {
        return Err(protocol_error(
            "catalog manifest resolution requires command bidirectional stream",
        ));
    }

    validate_transport_surface(frame, TransportSurface::ReliableStream(stream_role))?;

    if frame.header.frame_type != FrameType::ContractRequest {
        return Err(protocol_error(
            "catalog manifest resolution route requires ContractRequest frame",
        ));
    }

    let surface_plane = metadata.surface_plane();
    if !surface_plane.permits_family(frame.header.frame_type.frame_family()) {
        return Err(protocol_error(
            "catalog manifest resolution frame family not permitted on surface plane",
        ));
    }
    validate_catalog_route_admission(metadata)?;

    let Some(session_id) = metadata.session_id() else {
        return Err(security_error(
            "catalog manifest resolution requires authenticated session id",
        ));
    };

    if session_id != frame.header.session_id {
        return Err(protocol_error(
            "catalog manifest resolution session id does not match endpoint metadata",
        ));
    }

    Ok(CatalogManifestRouteAdmission {
        surface_plane,
        certificate_identity: metadata.certificate_identity().cloned(),
    })
}

fn validate_catalog_route_admission(metadata: &TransportEndpointMetadata) -> AndromedaResult<()> {
    if metadata.surface_plane() != SurfacePlane::Administration {
        return Err(security_error(
            "catalog manifest resolution requires Administration surface",
        ));
    }

    let Some(identity) = metadata.certificate_identity() else {
        return Err(security_error(
            "catalog manifest resolution requires bound certificate identity",
        ));
    };

    if identity.surface_scope() != SurfaceScope::Administration {
        return Err(security_error(
            "catalog manifest resolution certificate scope must match Administration surface",
        ));
    }

    Ok(())
}
