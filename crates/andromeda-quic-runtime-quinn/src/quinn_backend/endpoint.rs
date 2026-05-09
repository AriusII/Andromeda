use std::net::UdpSocket;
use std::sync::Arc;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub(super) fn endpoint_from_socket(
    server_config: Option<quinn::ServerConfig>,
    socket: UdpSocket,
) -> AndromedaResult<quinn::Endpoint> {
    quinn::Endpoint::new(
        Default::default(),
        server_config,
        socket,
        Arc::new(quinn::TokioRuntime),
    )
    .map_err(|e| transport_error(format!("failed to create Quinn endpoint: {e}")))
}

pub(super) fn transport_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Transport, message)
}
