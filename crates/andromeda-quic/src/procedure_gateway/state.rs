use andromeda_error::AndromedaResult;
use andromeda_principal::CertificateIdentity;
use andromeda_types::InvocationId;

use andromeda_rpc_protocol::FrameBytes;

use crate::{Connection, LifecycleState, SurfacePlane};

use super::errors::{protocol_error, security_error};

pub(super) struct GatewayState<'a> {
    pub(super) certificate_identity: &'a CertificateIdentity,
    pub(super) plane: SurfacePlane,
}

pub(super) fn resolve_gateway_state(connection: &Connection) -> AndromedaResult<GatewayState<'_>> {
    let certificate_identity = connection
        .certificate_identity()
        .ok_or_else(|| security_error("procedure gateway requires bound certificate identity"))?;
    let plane = connection.surface_plane();
    validate_certificate_scope(certificate_identity, plane)?;
    validate_application_gateway_plane(plane)?;

    Ok(GatewayState {
        certificate_identity,
        plane,
    })
}

pub(super) fn invocation_id_for_stream(stream_id: u64) -> InvocationId {
    InvocationId::new(stream_id)
}

pub(super) fn validate_dispatch_preconditions(connection: &Connection) -> AndromedaResult<()> {
    if connection.state() != LifecycleState::Active {
        return Err(protocol_error(format!(
            "cannot dispatch invocation: connection is in {:?} state, not Active",
            connection.state()
        )));
    }

    if connection.certificate_identity().is_none() {
        return Err(security_error(
            "cannot dispatch invocation: no certificate identity bound to connection",
        ));
    }

    Ok(())
}

pub(super) fn validate_frame_session_binding(
    connection: &Connection,
    frame: &FrameBytes,
) -> AndromedaResult<()> {
    match connection.session_id() {
        Some(session_id) if session_id == frame.header.session_id => Ok(()),
        Some(_) => Err(protocol_error(
            "procedure invocation frame session id does not match authenticated connection",
        )),
        None => Err(protocol_error(
            "procedure invocation requires authenticated connection session id",
        )),
    }
}

fn validate_application_gateway_plane(plane: SurfacePlane) -> AndromedaResult<()> {
    if !plane.is_application() {
        return Err(security_error(
            "ProcedureGateway is Application-surface only; administration, HA/DR, recovery, and monitoring work must use their dedicated surfaces",
        ));
    }

    Ok(())
}

fn validate_certificate_scope(
    certificate_identity: &CertificateIdentity,
    plane: SurfacePlane,
) -> AndromedaResult<()> {
    let required_scope = crate::mtls_identity::plane_to_required_surface_scope(plane);
    if certificate_identity.surface_scope() != required_scope {
        return Err(security_error(
            "certificate surface scope does not match connection plane",
        ));
    }

    Ok(())
}
