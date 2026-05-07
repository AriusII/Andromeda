use andromeda_core::{AndromedaResult, SessionId};
use andromeda_observe::CertificateIdentity;

use crate::{
    FrameBytes, FrameFamily, FrameType,
    rpc_dispatch::{FrameDispatch, dispatch_frame},
};

use super::{
    cancellation::{CancellationCause, CancellationOutcome, CancellationSignal},
    error::protocol_error,
    lifecycle::LifecycleState,
    surface::SurfacePlane,
};

/// V0 QUIC session connection state machine.
///
/// This type is intentionally synchronous and side-effect free. It models the
/// transport contract used by listeners and integration harnesses to assert
/// correct gating without binding to a concrete I/O runtime.
///
/// ## Certificate Identity Binding
///
/// A connection optionally carries a bound mTLS certificate identity extracted
/// from the peer certificate during handshake. Once bound, the identity is
/// immutable and used for dispatch authorization. The certificate's surface
/// scope must match the connection's surface plane.
#[derive(Debug)]
pub struct Connection {
    plane: SurfacePlane,
    state: LifecycleState,
    session_id: Option<SessionId>,
    certificate_identity: Option<CertificateIdentity>,
}

impl Connection {
    /// Creates a new connection bound to a surface plane in the `Hello` state.
    pub const fn new(plane: SurfacePlane) -> Self {
        Self {
            plane,
            state: LifecycleState::Hello,
            session_id: None,
            certificate_identity: None,
        }
    }

    /// Returns the surface plane this connection is bound to.
    pub const fn surface_plane(&self) -> SurfacePlane {
        self.plane
    }

    /// Returns the current lifecycle state.
    pub const fn state(&self) -> LifecycleState {
        self.state
    }

    /// Returns the negotiated session id, if the handshake has reached `Auth`.
    pub const fn session_id(&self) -> Option<SessionId> {
        self.session_id
    }

    /// Returns the mTLS certificate identity bound to this session, if present.
    ///
    /// Listener/runtime code must bind identity before dispatch authorization.
    pub const fn certificate_identity(&self) -> Option<&CertificateIdentity> {
        self.certificate_identity.as_ref()
    }

    /// Bind a mTLS certificate identity to this session.
    ///
    /// This method is called early in the handshake (before or after `accept_hello()`)
    /// to attach the extracted peer certificate identity. The identity's surface
    /// scope must match this connection's surface plane, or an error is returned.
    ///
    /// Once bound, the identity is immutable. Attempting to bind again returns an
    /// error to prevent accidental identity replacement.
    ///
    /// Returns `Err` if:
    /// - An identity is already bound to this session.
    /// - The certificate's surface scope does not match the connection's plane.
    pub fn set_certificate_identity(
        &mut self,
        identity: CertificateIdentity,
    ) -> AndromedaResult<()> {
        if self.certificate_identity.is_some() {
            return Err(protocol_error(
                "certificate identity is already bound to this session",
            ));
        }

        // Validate that the certificate's surface scope matches the connection plane.
        let required_scope = crate::mtls_identity::plane_to_required_surface_scope(self.plane);
        if identity.surface as u8 != required_scope as u8 {
            return Err(protocol_error(
                "certificate surface scope does not match connection plane",
            ));
        }

        self.certificate_identity = Some(identity);
        Ok(())
    }

    /// Returns true if RPC dispatch is currently permitted (state == Active).
    pub const fn is_active(&self) -> bool {
        matches!(self.state, LifecycleState::Active)
    }

    /// Accept a `Hello` frame, advancing `Hello -> Auth`.
    pub fn accept_hello(&mut self, frame: &FrameBytes) -> AndromedaResult<()> {
        if self.state != LifecycleState::Hello {
            return Err(protocol_error("Hello frame received outside Hello state"));
        }
        if frame.header.frame_type != FrameType::Hello {
            return Err(protocol_error("expected Hello frame to open the session"));
        }
        self.session_id = Some(frame.header.session_id);
        self.state = LifecycleState::Auth;
        Ok(())
    }

    /// Accept an `Auth` frame, advancing `Auth -> Active`.
    pub fn accept_auth(&mut self, frame: &FrameBytes) -> AndromedaResult<()> {
        if self.state != LifecycleState::Auth {
            return Err(protocol_error("Auth frame received outside Auth state"));
        }
        if frame.header.frame_type != FrameType::Auth {
            return Err(protocol_error(
                "expected Auth frame to complete the handshake",
            ));
        }
        if let Some(sid) = self.session_id
            && sid != frame.header.session_id
        {
            return Err(protocol_error("Auth frame session id does not match Hello"));
        }
        self.state = LifecycleState::Active;
        Ok(())
    }

    /// Dispatch a non-handshake frame on a declared surface plane.
    ///
    /// The caller asserts the operational plane of the request (e.g. an admin
    /// listener tags its inbound traffic as [`SurfacePlane::Administration`]).
    /// Cross-plane requests are rejected with a typed protocol error.
    pub fn dispatch(
        &mut self,
        frame: &FrameBytes,
        requested_plane: SurfacePlane,
    ) -> AndromedaResult<FrameDispatch> {
        // Plane match is checked before state to give callers a precise
        // diagnostic for surface mismatch even on Hello/Auth connections.
        if requested_plane != self.plane {
            return Err(protocol_error(
                "frame dispatched on wrong surface plane for this connection",
            ));
        }

        let family = frame.header.frame_type.frame_family();
        if !self.plane.permits_family(family) {
            return Err(protocol_error(
                "frame family not permitted on this surface plane",
            ));
        }

        match self.state {
            LifecycleState::Hello | LifecycleState::Auth => Err(protocol_error(
                "RPC dispatch attempted before session handshake completed",
            )),
            LifecycleState::Closed => {
                Err(protocol_error("RPC dispatch attempted on a closed session"))
            }
            LifecycleState::Draining => {
                // While draining, new command/contract requests are refused;
                // result-stream and diagnostic frames may still flow.
                if matches!(
                    family,
                    FrameFamily::RpcCommand | FrameFamily::ContractControl
                ) {
                    return Err(protocol_error(
                        "new commands rejected while session is draining",
                    ));
                }
                self.validate_dispatch_session(frame)?;
                let role = frame.header.frame_type.stream_role();
                dispatch_frame(frame, role)
            }
            LifecycleState::Active => {
                self.validate_dispatch_session(frame)?;
                let role = frame.header.frame_type.stream_role();
                dispatch_frame(frame, role)
            }
        }
    }

    fn validate_dispatch_session(&self, frame: &FrameBytes) -> AndromedaResult<()> {
        match self.session_id {
            Some(session_id) if session_id == frame.header.session_id => Ok(()),
            Some(_) => Err(protocol_error(
                "frame session id does not match authenticated connection",
            )),
            None => Err(protocol_error(
                "frame dispatch requires authenticated connection session id",
            )),
        }
    }

    /// Begin draining the session. `Active -> Draining`.
    pub fn begin_drain(&mut self) -> AndromedaResult<()> {
        match self.state {
            LifecycleState::Active => {
                self.state = LifecycleState::Draining;
                Ok(())
            }
            LifecycleState::Draining => Ok(()),
            LifecycleState::Closed => Err(protocol_error("cannot drain a closed session")),
            LifecycleState::Hello | LifecycleState::Auth => {
                Err(protocol_error("cannot drain a session before it is active"))
            }
        }
    }

    /// Force-close the session. Always succeeds; subsequent dispatches fail.
    pub fn close(&mut self) {
        self.state = LifecycleState::Closed;
    }

    /// Route a typed cancellation signal through the lifecycle gate.
    ///
    /// Cancellation is a control-plane concept and is intentionally modeled
    /// here without inventing a new wire frame: callers feed a parsed
    /// [`CancellationSignal`] through this method to obtain a deterministic,
    /// typed outcome. The signal must address the session that owns the
    /// in-flight request (`signal.session_id` must match the negotiated
    /// session id once the handshake has reached `Auth`).
    ///
    /// Routing rules:
    ///
    /// * `Hello` / `Auth` -> `AndromedaErrorKind::Protocol`
    ///   ("cancellation before handshake completed").
    /// * `Closed` -> `AndromedaErrorKind::Protocol`
    ///   ("cancellation on a closed session").
    /// * `Active` -> [`CancellationOutcome::Delivered`].
    /// * `Draining` -> [`CancellationOutcome::DeliveredDuringDrain`] for
    ///   client/admin/timeout causes; server-originated `SessionClosed`
    ///   while still draining is rejected as a protocol error.
    pub fn route_cancellation(
        &self,
        signal: &CancellationSignal,
    ) -> AndromedaResult<CancellationOutcome> {
        match self.state {
            LifecycleState::Hello | LifecycleState::Auth => Err(protocol_error(
                "cancellation received before session handshake completed",
            )),
            LifecycleState::Closed => {
                Err(protocol_error("cancellation received on a closed session"))
            }
            LifecycleState::Active | LifecycleState::Draining => {
                if let Some(sid) = self.session_id
                    && sid != signal.session_id
                {
                    return Err(protocol_error(
                        "cancellation session id does not match active session",
                    ));
                }
                match (self.state, signal.cause) {
                    (LifecycleState::Draining, CancellationCause::SessionClosed) => Err(
                        protocol_error("SessionClosed cancellation requires Closed state"),
                    ),
                    (LifecycleState::Active, _) => Ok(CancellationOutcome::Delivered),
                    (LifecycleState::Draining, _) => Ok(CancellationOutcome::DeliveredDuringDrain),
                    (LifecycleState::Hello | LifecycleState::Auth | LifecycleState::Closed, _) => {
                        Err(protocol_error(
                            "cancellation routing observed an invalid lifecycle state",
                        ))
                    }
                }
            }
        }
    }
}
