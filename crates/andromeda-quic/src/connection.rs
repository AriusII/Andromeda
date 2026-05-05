//! Connection lifecycle and surface plane gating.
//!
//! Implements the V0 QUIC session state machine:
//!
//! ```text
//!                ┌────────┐  hello ok    ┌──────┐  auth ok    ┌────────┐
//!   new() ─────▶ │ Hello  │ ───────────▶ │ Auth │ ──────────▶ │ Active │
//!                └────────┘              └──────┘             └───┬────┘
//!                     │                      │                   │
//!                     │                      │                   │ begin_drain()
//!                     │                      │                   ▼
//!                     │                      │              ┌──────────┐
//!                     │                      │              │ Draining │
//!                     │                      │              └────┬─────┘
//!                     ▼                      ▼                   ▼
//!                                ┌────────┐
//!                                │ Closed │ ◀──── close()
//!                                └────────┘
//! ```
//!
//! The state machine is intentionally non-network-aware: it operates on parsed
//! `FrameBytes` and produces typed protocol errors. Wiring to a real QUIC
//! socket layer is performed elsewhere; this module is the contract surface.
//!
//! Surface planes partition session intent. A connection is bound to exactly
//! one plane at construction. RPC dispatch declares the operational class of
//! the request; cross-plane requests (e.g. an Administration command on an
//! Application connection) are rejected with [`AndromedaErrorKind::Protocol`].

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId};
use andromeda_observe::CertificateIdentity;

use crate::{
    FrameBytes, FrameFamily, FrameType,
    rpc_dispatch::{FrameDispatch, dispatch_frame},
};

/// QUIC/TLS early-data admission policy for a surface listener.
///
/// V0 intentionally exposes only `Disabled`: until a later decision scopes
/// replay-safe behavior, listener runtime wiring must not admit 0-RTT traffic
/// for any plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EarlyDataPolicy {
    /// Reject QUIC/TLS 0-RTT early data.
    Disabled,
}

/// QUIC datagram admission policy for a surface listener.
///
/// The V0 transport model is stream/frame based. Datagrams are not available
/// as a shortcut for WAL shipping, HA/DR state transfer, or generic telemetry
/// bypass paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatagramPolicy {
    /// QUIC datagrams are disabled for this listener.
    Disabled,
}

/// Surface plane bound to a QUIC session at handshake time.
///
/// The plane fixes the operational tenancy of every frame admitted on the
/// connection. It is a purely typed value; concrete plane→port routing is the
/// responsibility of the listener layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfacePlane {
    /// Tenant-facing application traffic (read/write, contracts, RPC).
    Application,
    /// Operator administration: catalog, schema, governance, drain commands.
    Administration,
    /// HADR / cluster control plane (replication, failover, cluster membership).
    HighAvailability,
    /// Read-only monitoring and diagnostics surface.
    Monitoring,
}

impl SurfacePlane {
    /// Returns true if this plane permits a frame family at the transport level.
    ///
    /// Session control and diagnostic frames are allowed on every plane so
    /// that handshake and error reporting always remain reachable.
    pub const fn permits_family(self, family: FrameFamily) -> bool {
        matches!(
            (self, family),
            (_, FrameFamily::SessionControl)
                | (_, FrameFamily::Diagnostic)
                | (Self::Application, FrameFamily::ContractControl)
                | (Self::Application, FrameFamily::RpcCommand)
                | (Self::Application, FrameFamily::RpcResultStream)
                | (Self::Application, FrameFamily::Telemetry)
                | (Self::Administration, FrameFamily::ContractControl)
                | (Self::Administration, FrameFamily::RpcCommand)
                | (Self::Administration, FrameFamily::RpcResultStream)
                | (Self::Administration, FrameFamily::Telemetry)
                | (Self::HighAvailability, FrameFamily::ContractControl)
                | (Self::HighAvailability, FrameFamily::RpcCommand)
                | (Self::HighAvailability, FrameFamily::RpcResultStream)
                | (Self::HighAvailability, FrameFamily::Telemetry)
                | (Self::Monitoring, FrameFamily::Telemetry)
        )
    }
}

/// Runtime-free listener/session contract for one surface plane.
///
/// This type is the D2 scaffold consumed by future `runtime-quinn` listener
/// wiring. It deliberately contains no socket address, certificate, executor,
/// or stream-manager state: those belong to D3-D5. A listener owns exactly one
/// [`SurfacePlane`], always disables early data, and opens sessions that are
/// bound to the listener plane at construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceListenerConfig {
    plane: SurfacePlane,
    early_data: EarlyDataPolicy,
    datagrams: DatagramPolicy,
}

impl SurfaceListenerConfig {
    /// Builds the listener/session contract for one surface plane.
    pub const fn for_plane(plane: SurfacePlane) -> Self {
        Self {
            plane,
            early_data: EarlyDataPolicy::Disabled,
            datagrams: DatagramPolicy::Disabled,
        }
    }

    /// Builds the Application-plane listener/session contract.
    pub const fn application() -> Self {
        Self::for_plane(SurfacePlane::Application)
    }

    /// Builds the Administration-plane listener/session contract.
    pub const fn administration() -> Self {
        Self::for_plane(SurfacePlane::Administration)
    }

    /// Builds the HA/DR-plane listener/session contract.
    pub const fn high_availability() -> Self {
        Self::for_plane(SurfacePlane::HighAvailability)
    }

    /// Builds the Monitoring-plane listener/session contract.
    pub const fn monitoring() -> Self {
        Self::for_plane(SurfacePlane::Monitoring)
    }

    /// Returns the single plane accepted by this listener.
    pub const fn surface_plane(&self) -> SurfacePlane {
        self.plane
    }

    /// Returns the listener early-data policy.
    pub const fn early_data_policy(&self) -> EarlyDataPolicy {
        self.early_data
    }

    /// Returns the listener datagram policy.
    pub const fn datagram_policy(&self) -> DatagramPolicy {
        self.datagrams
    }

    /// Creates a runtime-free session model bound to this listener's plane.
    pub const fn new_connection(&self) -> Connection {
        Connection::new(self.plane)
    }
}

/// Complete D2 listener-per-plane configuration set.
///
/// Future executable listener wiring should create one concrete listener from
/// each field rather than multiplexing planes through a generic command
/// surface. Certificate identity, async executor handles, and stream runtime
/// management are intentionally excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceListenerSet {
    application: SurfaceListenerConfig,
    administration: SurfaceListenerConfig,
    high_availability: SurfaceListenerConfig,
    monitoring: SurfaceListenerConfig,
}

impl SurfaceListenerSet {
    /// Returns the canonical four-listener surface configuration.
    pub const fn standard() -> Self {
        Self {
            application: SurfaceListenerConfig::application(),
            administration: SurfaceListenerConfig::administration(),
            high_availability: SurfaceListenerConfig::high_availability(),
            monitoring: SurfaceListenerConfig::monitoring(),
        }
    }

    /// Application-plane listener/session configuration.
    pub const fn application(&self) -> SurfaceListenerConfig {
        self.application
    }

    /// Administration-plane listener/session configuration.
    pub const fn administration(&self) -> SurfaceListenerConfig {
        self.administration
    }

    /// HA/DR-plane listener/session configuration.
    pub const fn high_availability(&self) -> SurfaceListenerConfig {
        self.high_availability
    }

    /// Monitoring-plane listener/session configuration.
    pub const fn monitoring(&self) -> SurfaceListenerConfig {
        self.monitoring
    }
}

/// V0 connection lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleState {
    /// Awaiting client `Hello` frame.
    Hello,
    /// `Hello` accepted; awaiting `Auth` frame.
    Auth,
    /// Authenticated session; RPC and contract frames may be dispatched.
    Active,
    /// Drain initiated; new commands are rejected, in-flight result streams
    /// may continue to completion.
    Draining,
    /// Session terminated. Any further frames are protocol errors.
    Closed,
}

/// V0 QUIC session connection state machine.
///
/// This type is intentionally synchronous and side-effect free. It models the
/// transport contract used by listeners and integration harnesses to assert
/// correct gating without binding to a concrete I/O runtime.
///
/// ## D3: Certificate Identity Binding
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
    /// D3 establishes that certificate identity is extracted from the peer's
    /// mTLS certificate and bound before dispatch authorization. This method
    /// allows callers (listeners, authorization gates) to access the identity.
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

    /// Accept a `Hello` frame, advancing `Hello → Auth`.
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

    /// Accept an `Auth` frame, advancing `Auth → Active`.
    pub fn accept_auth(&mut self, frame: &FrameBytes) -> AndromedaResult<()> {
        if self.state != LifecycleState::Auth {
            return Err(protocol_error("Auth frame received outside Auth state"));
        }
        if frame.header.frame_type != FrameType::Auth {
            return Err(protocol_error(
                "expected Auth frame to complete the handshake",
            ));
        }
        if let Some(sid) = self.session_id {
            if sid != frame.header.session_id {
                return Err(protocol_error("Auth frame session id does not match Hello"));
            }
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
                let role = frame.header.frame_type.stream_role();
                dispatch_frame(frame, role)
            }
            LifecycleState::Active => {
                let role = frame.header.frame_type.stream_role();
                dispatch_frame(frame, role)
            }
        }
    }

    /// Begin draining the session. `Active → Draining`.
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
    /// * `Hello` / `Auth` → [`AndromedaErrorKind::Protocol`]
    ///   ("cancellation before handshake completed").
    /// * `Closed` → [`AndromedaErrorKind::Protocol`]
    ///   ("cancellation on a closed session").
    /// * `Active` → [`CancellationOutcome::Delivered`].
    /// * `Draining` → [`CancellationOutcome::DeliveredDuringDrain`] for
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
                if let Some(sid) = self.session_id {
                    if sid != signal.session_id {
                        return Err(protocol_error(
                            "cancellation session id does not match active session",
                        ));
                    }
                }
                match (self.state, signal.cause) {
                    (LifecycleState::Draining, CancellationCause::SessionClosed) => Err(
                        protocol_error("SessionClosed cancellation requires Closed state"),
                    ),
                    (LifecycleState::Active, _) => Ok(CancellationOutcome::Delivered),
                    (LifecycleState::Draining, _) => Ok(CancellationOutcome::DeliveredDuringDrain),
                    _ => unreachable!(),
                }
            }
        }
    }
}

/// Origin of a cancellation signal.
///
/// The cause is part of the typed protocol surface: it lets dispatchers
/// distinguish a client-initiated abort from a server-side timeout or
/// administrative kill, and lets the lifecycle gate reject server-only
/// causes when they arrive in the wrong state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CancellationCause {
    /// Client explicitly cancelled the in-flight request.
    ClientRequested,
    /// Per-request timeout fired.
    Timeout,
    /// Administrative abort issued out-of-band (e.g. operator drain).
    AdminAbort,
    /// Server is draining the session; in-flight commands are being
    /// terminated as part of the drain protocol.
    SessionDraining,
    /// Server has closed the session; emitted as a final terminator.
    SessionClosed,
}

/// Typed cancellation control message.
///
/// Cancellation is request-scoped and session-scoped: it must always carry
/// a [`RequestId`] (cancellation of "everything on the session" is modeled
/// at the lifecycle layer via [`Connection::begin_drain`] and
/// [`Connection::close`], not here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancellationSignal {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub cause: CancellationCause,
}

/// Outcome of routing a cancellation signal through the lifecycle gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationOutcome {
    /// The session is `Active` and the signal targets an in-flight request.
    Delivered,
    /// The session is `Draining`; the signal is permitted because in-flight
    /// requests are still allowed to complete or be aborted during drain.
    DeliveredDuringDrain,
}

fn protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{RequestId, SessionId};

    use crate::frame::{FRAME_HEADER_CRC_UNCHECKED, FrameHeader};

    fn frame(frame_type: FrameType, session: u64) -> FrameBytes {
        let payload = match frame_type {
            FrameType::RpcExecuteRequest | FrameType::RpcBatch => b"x".to_vec(),
            _ => Vec::new(),
        };
        FrameBytes {
            header: FrameHeader {
                frame_type,
                request_id: RequestId::new(1),
                session_id: SessionId::new(session),
                tx_id: None,
                payload_length: payload.len() as u64,
                flags: 0,
                header_crc: FRAME_HEADER_CRC_UNCHECKED,
            },
            payload,
        }
    }

    #[test]
    fn new_connection_starts_in_hello_state() {
        let conn = Connection::new(SurfacePlane::Application);
        assert_eq!(conn.state(), LifecycleState::Hello);
        assert_eq!(conn.surface_plane(), SurfacePlane::Application);
        assert!(conn.session_id().is_none());
        assert!(!conn.is_active());
    }

    #[test]
    fn rpc_dispatch_rejected_before_handshake() {
        let mut conn = Connection::new(SurfacePlane::Application);
        let exec = frame(FrameType::RpcExecuteRequest, 7);
        let err = conn.dispatch(&exec, SurfacePlane::Application).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn handshake_progresses_hello_then_auth_then_active() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 42)).unwrap();
        assert_eq!(conn.state(), LifecycleState::Auth);
        assert_eq!(conn.session_id(), Some(SessionId::new(42)));

        conn.accept_auth(&frame(FrameType::Auth, 42)).unwrap();
        assert_eq!(conn.state(), LifecycleState::Active);
        assert!(conn.is_active());

        let exec = frame(FrameType::RpcExecuteRequest, 42);
        let dispatch = conn.dispatch(&exec, SurfacePlane::Application).unwrap();
        assert_eq!(dispatch.frame_type, FrameType::RpcExecuteRequest);
    }

    #[test]
    fn auth_rejected_when_session_id_mismatches_hello() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 1)).unwrap();
        let err = conn.accept_auth(&frame(FrameType::Auth, 2)).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn auth_frame_in_hello_state_is_rejected() {
        let mut conn = Connection::new(SurfacePlane::Application);
        let err = conn.accept_auth(&frame(FrameType::Auth, 1)).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn surface_mismatch_is_rejected_with_protocol_error() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 9)).unwrap();
        conn.accept_auth(&frame(FrameType::Auth, 9)).unwrap();
        let exec = frame(FrameType::RpcExecuteRequest, 9);
        let err = conn
            .dispatch(&exec, SurfacePlane::Administration)
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn monitoring_plane_rejects_rpc_command_family() {
        let mut conn = Connection::new(SurfacePlane::Monitoring);
        conn.accept_hello(&frame(FrameType::Hello, 5)).unwrap();
        conn.accept_auth(&frame(FrameType::Auth, 5)).unwrap();
        let exec = frame(FrameType::RpcExecuteRequest, 5);
        let err = conn.dispatch(&exec, SurfacePlane::Monitoring).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

        let telemetry = frame(FrameType::TelemetrySoftSignal, 5);
        assert!(conn.dispatch(&telemetry, SurfacePlane::Monitoring).is_ok());
    }

    #[test]
    fn drain_rejects_new_commands_but_allows_result_frames() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 3)).unwrap();
        conn.accept_auth(&frame(FrameType::Auth, 3)).unwrap();
        conn.begin_drain().unwrap();
        assert_eq!(conn.state(), LifecycleState::Draining);

        let exec = frame(FrameType::RpcExecuteRequest, 3);
        let err = conn.dispatch(&exec, SurfacePlane::Application).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

        // Result-stream frames still flow during drain.
        let batch = frame(FrameType::RpcBatch, 3);
        assert!(conn.dispatch(&batch, SurfacePlane::Application).is_ok());
    }

    #[test]
    fn close_terminates_session_and_blocks_dispatch() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 11)).unwrap();
        conn.accept_auth(&frame(FrameType::Auth, 11)).unwrap();
        conn.close();
        assert_eq!(conn.state(), LifecycleState::Closed);

        let exec = frame(FrameType::RpcExecuteRequest, 11);
        let err = conn.dispatch(&exec, SurfacePlane::Application).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

        // Drain after close is a protocol error.
        assert_eq!(
            conn.begin_drain().unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn drain_before_active_is_protocol_error() {
        let mut conn = Connection::new(SurfacePlane::Application);
        let err = conn.begin_drain().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn surface_plane_permits_family_matrix() {
        assert!(SurfacePlane::Application.permits_family(FrameFamily::RpcCommand));
        assert!(SurfacePlane::Administration.permits_family(FrameFamily::ContractControl));
        assert!(SurfacePlane::HighAvailability.permits_family(FrameFamily::RpcCommand));
        assert!(SurfacePlane::Monitoring.permits_family(FrameFamily::Telemetry));
        assert!(!SurfacePlane::Monitoring.permits_family(FrameFamily::RpcCommand));
        assert!(!SurfacePlane::Monitoring.permits_family(FrameFamily::ContractControl));
        // Session control and diagnostic always pass on every plane.
        assert!(SurfacePlane::Application.permits_family(FrameFamily::SessionControl));
        assert!(SurfacePlane::Monitoring.permits_family(FrameFamily::SessionControl));
        assert!(SurfacePlane::Monitoring.permits_family(FrameFamily::Diagnostic));
    }

    fn cancel(session: u64, cause: CancellationCause) -> CancellationSignal {
        CancellationSignal {
            request_id: RequestId::new(99),
            session_id: SessionId::new(session),
            cause,
        }
    }

    #[test]
    fn cancellation_before_handshake_is_protocol_error() {
        let conn = Connection::new(SurfacePlane::Application);
        let err = conn
            .route_cancellation(&cancel(1, CancellationCause::ClientRequested))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn cancellation_in_auth_state_is_protocol_error() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 1)).unwrap();
        let err = conn
            .route_cancellation(&cancel(1, CancellationCause::ClientRequested))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn cancellation_in_active_state_is_delivered() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 5)).unwrap();
        conn.accept_auth(&frame(FrameType::Auth, 5)).unwrap();
        let outcome = conn
            .route_cancellation(&cancel(5, CancellationCause::Timeout))
            .unwrap();
        assert_eq!(outcome, CancellationOutcome::Delivered);
    }

    #[test]
    fn cancellation_session_id_mismatch_is_rejected() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 5)).unwrap();
        conn.accept_auth(&frame(FrameType::Auth, 5)).unwrap();
        let err = conn
            .route_cancellation(&cancel(6, CancellationCause::ClientRequested))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn cancellation_during_drain_is_delivered_with_drain_outcome() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 9)).unwrap();
        conn.accept_auth(&frame(FrameType::Auth, 9)).unwrap();
        conn.begin_drain().unwrap();

        let outcome = conn
            .route_cancellation(&cancel(9, CancellationCause::AdminAbort))
            .unwrap();
        assert_eq!(outcome, CancellationOutcome::DeliveredDuringDrain);
    }

    #[test]
    fn session_closed_cause_during_drain_is_protocol_error() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 9)).unwrap();
        conn.accept_auth(&frame(FrameType::Auth, 9)).unwrap();
        conn.begin_drain().unwrap();

        let err = conn
            .route_cancellation(&cancel(9, CancellationCause::SessionClosed))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn cancellation_on_closed_session_is_protocol_error() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&frame(FrameType::Hello, 9)).unwrap();
        conn.accept_auth(&frame(FrameType::Auth, 9)).unwrap();
        conn.close();
        let err = conn
            .route_cancellation(&cancel(9, CancellationCause::ClientRequested))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    // ========================================================================
    // D3: Certificate Identity Binding Contract Tests
    // ========================================================================

    #[test]
    fn certificate_identity_binding_succeeds_when_scope_matches() {
        use andromeda_observe::SurfaceScope;

        let mut conn = Connection::new(SurfacePlane::Application);
        let identity = CertificateIdentity::new(
            "a".repeat(64),            // SHA256 fingerprint
            "test-service",            // CN
            SurfaceScope::Application, // Must match Application plane
        )
        .unwrap();

        assert!(conn.set_certificate_identity(identity).is_ok());
        assert!(conn.certificate_identity().is_some());
    }

    #[test]
    fn certificate_identity_binding_rejects_scope_mismatch() {
        use andromeda_observe::SurfaceScope;

        let mut conn = Connection::new(SurfacePlane::Application);
        let identity = CertificateIdentity::new(
            "a".repeat(64),
            "test-admin",
            SurfaceScope::Administration, // Mismatch: admin cert on app plane
        )
        .unwrap();

        let err = conn.set_certificate_identity(identity).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
        assert!(conn.certificate_identity().is_none());
    }

    #[test]
    fn certificate_identity_binding_is_immutable() {
        use andromeda_observe::SurfaceScope;

        let mut conn = Connection::new(SurfacePlane::Application);
        let identity1 =
            CertificateIdentity::new("a".repeat(64), "svc1", SurfaceScope::Application).unwrap();
        let identity2 =
            CertificateIdentity::new("b".repeat(64), "svc2", SurfaceScope::Application).unwrap();

        conn.set_certificate_identity(identity1).unwrap();
        let err = conn.set_certificate_identity(identity2).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

        // First identity remains.
        assert_eq!(
            conn.certificate_identity().unwrap().fingerprint,
            "a".repeat(64)
        );
    }

    #[test]
    fn certificate_identity_persists_across_lifecycle() {
        use andromeda_observe::SurfaceScope;

        let mut conn = Connection::new(SurfacePlane::Administration);
        let identity = CertificateIdentity::new(
            "c".repeat(64),
            "admin-operator",
            SurfaceScope::Administration,
        )
        .unwrap();

        conn.set_certificate_identity(identity.clone()).unwrap();
        conn.accept_hello(&frame(FrameType::Hello, 1)).unwrap();
        conn.accept_auth(&frame(FrameType::Auth, 1)).unwrap();

        // Identity is still present and unchanged.
        assert_eq!(conn.certificate_identity().unwrap(), &identity);
        assert_eq!(conn.state(), LifecycleState::Active);
    }

    #[test]
    fn ha_dr_plane_requires_cluster_scope() {
        use andromeda_observe::SurfaceScope;

        let mut conn = Connection::new(SurfacePlane::HighAvailability);
        let identity = CertificateIdentity::new(
            "d".repeat(64),
            "cluster-node",
            SurfaceScope::Cluster, // Correct for HA/DR
        )
        .unwrap();

        assert!(conn.set_certificate_identity(identity).is_ok());

        // Wrong scope should be rejected.
        let mut conn2 = Connection::new(SurfacePlane::HighAvailability);
        let wrong_identity =
            CertificateIdentity::new("e".repeat(64), "app-svc", SurfaceScope::Application).unwrap();
        assert!(conn2.set_certificate_identity(wrong_identity).is_err());
    }

    #[test]
    fn monitoring_plane_requires_monitoring_agent_scope() {
        use andromeda_observe::SurfaceScope;

        let mut conn = Connection::new(SurfacePlane::Monitoring);
        let identity = CertificateIdentity::new(
            "f".repeat(64),
            "telemetry-agent",
            SurfaceScope::MonitoringAgent, // Correct for Monitoring
        )
        .unwrap();

        assert!(conn.set_certificate_identity(identity).is_ok());
    }
}
