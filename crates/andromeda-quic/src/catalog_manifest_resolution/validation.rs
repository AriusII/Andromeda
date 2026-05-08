use andromeda_core::AndromedaResult;
use andromeda_core::SurfaceScope;
use andromeda_proto::FrameEnvelope as ProtoFrameEnvelope;

use crate::{FrameBytes, SurfacePlane, TransportEndpointMetadata};

use super::errors::{protocol_error, security_error};
use super::gateway::CatalogManifestResolutionContext;
use super::manifest::CatalogManifestResolutionResponse;

pub(super) fn validate_catalog_route_admission(
    metadata: &TransportEndpointMetadata,
) -> AndromedaResult<()> {
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

pub(super) fn validate_request_context(
    frame: &FrameBytes,
    envelope: &ProtoFrameEnvelope,
    payload_request_id: u64,
) -> AndromedaResult<()> {
    if payload_request_id != frame.header.request_id.get()
        || payload_request_id != envelope.request_id.get()
    {
        return Err(protocol_error(
            "catalog manifest resolution request id changed across frame, envelope, and payload",
        ));
    }

    Ok(())
}

pub(super) fn validate_response_context(
    context: &CatalogManifestResolutionContext,
    response: &CatalogManifestResolutionResponse,
) -> AndromedaResult<()> {
    if response.request_id != context.request_id() {
        return Err(protocol_error(
            "catalog manifest resolution runtime changed response request id",
        ));
    }

    Ok(())
}
