//! D4: QUIC Connection-to-Executor Dispatch Bridge
//!
//! This module bridges the QUIC transport layer ([`Connection`], [`CertificateIdentity`],
//! [`SurfacePlane`]) to the execution layer (executor admission, invocation context,
//! result streams).
//!
//! ## Why it lives in `andromeda-exec`
//!
//! Per DEC-017 and the architecture in this crate, the bridge sits between two layers:
//! - **QUIC Transport** (from `andromeda-quic`): Connection, SurfacePlane, CertificateIdentity
//! - **Execution Gate** (in `andromeda-exec`): SurfaceAuthorizer, AuthorizedProcedureDispatch
//!
//! `andromeda-exec` is the only crate that already depends on both `andromeda-quic`
//! and the executor/authorization primitives, so it is the natural home for the bridge.
//! This avoids introducing a circular dependency or a new internal edge.
//!
//! ## Architecture
//!
//! The bridge enforces the authorization boundary:
//!
//! ```text
//! QUIC Transport                    Executor
//! ─────────────────────────────────────────────────
//! ┌─────────────────┐
//! │ QuicConnection  │
//! │ + plane         │
//! │ + cert_identity │
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌──────────────────────────────────────────────┐
//! │ ExecutorDispatchBridge                       │
//! │                                              │
//! │ 1. Validate cert scope ≈ plane              │
//! │ 2. Call SurfacePlaneAuthorizer               │
//! │ 3. If denied: emit pre-tx event, return err  │
//! │ 4. If allowed: create AuthorizedProcedureDispatch
//! │ 5. Map QUIC stream → InvocationRequest       │
//! │ 6. Pass to executor with token              │
//! │ 7. Encode result to frames                   │
//! └──────────────────────────────────────────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │ LocalExecutor    │
//! │ (admitted gate)  │
//! └─────────────────┘
//! ```
//!
//! ## Authorization Flow
//!
//! - **Certificate Identity Required**: The QUIC connection must have a bound
//!   certificate identity before dispatch. If absent, returns `AuthorizationError`.
//! - **Plane Scope Match**: The certificate's `surface_scope` is derived from
//!   the connection's [`SurfacePlane`] via `plane_to_required_surface_scope()`.
//! - **Pre-Transaction Rejection**: If authorization is denied, no executor
//!   invocation occurs. The bridge emits an `ExecutionTransitionTrace` with
//!   terminal code but no transaction/LSN evidence.
//! - **Stream Correlation**: QUIC stream ID is bound to [`InvocationId`] via
//!   a stable, deterministic mapping.
//!
//! ## Exclusions (V0)
//!
//! - No quinn connection types: this module uses the abstract [`Connection`].
//! - No actual TLS/mTLS wiring: certificate extraction is deferred to the
//!   `runtime-quinn` feature in D4 listener integration.
//! - No stream multiplexing: result-to-frame encoding is a contract surface,
//!   not implemented here (D5 concern).
//! - No gRPC: invocation is typed, not gRPC-mapped.

use crate::surface_gate::{AuthorizedProcedureDispatch, SurfacePlaneAuthorizer};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CertificateIdentity, InvocationId,
};
use andromeda_observe::{AuthorizationOutcome, TraceId};
use andromeda_quic::{Connection, SurfacePlane};

/// Bridges a QUIC connection and certificate identity to executor dispatch.
///
/// This type holds the connection state, certificate identity, and surface plane,
/// and provides methods to authorize and route invocation requests to
/// the executor layer.
///
/// ## Type Invariants
///
/// - The connection must be in `LifecycleState::Active` before dispatch.
/// - The certificate identity must be bound to the connection.
/// - The certificate scope must match the connection plane.
/// - Authorization must succeed before any executor invocation.
#[derive(Debug)]
pub struct ExecutorDispatchBridge<'a> {
    /// The QUIC connection, bound to a surface plane.
    connection: &'a Connection,

    /// The certificate identity extracted and bound during handshake (D3).
    ///
    /// This is the principal presented to the authorizer. Must not be None.
    certificate_identity: &'a CertificateIdentity,

    /// The surface plane this connection is bound to.
    ///
    /// Immutable; set at connection construction. Used to validate that
    /// certificate scope matches and to route to the correct executor.
    plane: SurfacePlane,
}

impl<'a> ExecutorDispatchBridge<'a> {
    /// Constructs a new bridge from a connection and its bound certificate identity.
    ///
    /// Returns `Err` if:
    /// - The connection has no bound certificate identity.
    /// - The certificate scope does not match the connection plane.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use andromeda_quic::{Connection, SurfacePlane};
    /// use andromeda_exec::ExecutorDispatchBridge;
    /// # use andromeda_core::CertificateIdentity;
    /// # use andromeda_core::SurfaceScope;
    ///
    /// # let mut conn = Connection::new(SurfacePlane::Application);
    /// # let identity = CertificateIdentity::new("a".repeat(64), "svc-001", SurfaceScope::Application).unwrap();
    /// # conn.set_certificate_identity(identity.clone()).unwrap();
    /// let bridge = ExecutorDispatchBridge::new(&conn)?;
    /// # Ok::<(), andromeda_core::AndromedaError>(())
    /// ```
    pub fn new(connection: &'a Connection) -> AndromedaResult<Self> {
        let certificate_identity = connection.certificate_identity().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Security,
                "executor dispatch bridge requires bound certificate identity",
            )
        })?;

        let plane = connection.surface_plane();

        // Validate scope match.
        let required_scope = andromeda_quic::mtls_identity::plane_to_required_surface_scope(plane);
        if certificate_identity.surface_scope() != required_scope {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "certificate surface scope does not match connection plane",
            ));
        }

        Ok(Self {
            connection,
            certificate_identity,
            plane,
        })
    }

    /// Returns the surface plane this connection is bound to.
    pub const fn surface_plane(&self) -> SurfacePlane {
        self.plane
    }

    /// Returns the certificate identity bound to this connection.
    pub const fn certificate_identity(&self) -> &'a CertificateIdentity {
        self.certificate_identity
    }

    /// Returns the underlying QUIC connection.
    pub const fn connection(&self) -> &'a Connection {
        self.connection
    }

    /// Authorize procedure dispatch using the bound certificate and connection plane.
    ///
    /// This is the primary authorization gate. It calls `SurfacePlaneAuthorizer::authorize_procedure_dispatch()`
    /// with the certificate fingerprint and plane. If authorization is denied, the denial is returned
    /// as an [`AuthorizationOutcome`] so callers can emit the embedded security audit trace.
    ///
    /// If authorization succeeds, an [`AuthorizedProcedureDispatch`] token is returned.
    /// This token proves that the dispatch was authorized before any executor invocation.
    ///
    /// # Security Properties
    ///
    /// - **Pre-transaction**: Authorization decisions are made *before* executor invocation.
    /// - **Plane-aware**: The authorizer enforces that only Application plane permits ExecuteProcedure.
    /// - **Auditable**: Both allow and deny outcomes carry security audit traces.
    /// - **No side effects**: This method does not create transactions, emit invocations, or modify state.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use andromeda_exec::ExecutorDispatchBridge;
    /// # use andromeda_observe::TraceId;
    /// # use andromeda_exec::SurfacePlaneAuthorizer;
    /// # use andromeda_core::{CertificateIdentity, SurfaceScope};
    /// # let mut conn = andromeda_quic::Connection::new(andromeda_quic::SurfacePlane::Application);
    /// # let identity = CertificateIdentity::new("a".repeat(64), "svc-001", SurfaceScope::Application).unwrap();
    /// # conn.set_certificate_identity(identity).unwrap();
    /// # let registry = andromeda_observe::PrincipalRegistry::new();
    /// # let bridge = ExecutorDispatchBridge::new(&conn)?;
    ///
    /// let authorizer = SurfacePlaneAuthorizer::new(&registry);
    /// let outcome = bridge.authorize_procedure_dispatch(
    ///     &authorizer,
    ///     TraceId::new(42),
    /// );
    ///
    /// match outcome {
    ///     Ok(Ok(token)) => {
    ///         // Authorization succeeded; token can be passed to executor.
    ///         println!("Authorized for plane: {:?}", token.plane());
    ///     }
    ///     Ok(Err(denial)) => {
    ///         // Authorization denied; emit audit trace and return error.
    ///         println!("Access denied: {:?}", denial);
    ///     }
    ///     Err(e) => {
    ///         // System error (e.g., principal not found).
    ///         println!("Authorization error: {}", e);
    ///     }
    /// }
    /// # Ok::<(), andromeda_core::AndromedaError>(())
    /// ```
    pub fn authorize_procedure_dispatch(
        &self,
        authorizer: &SurfacePlaneAuthorizer,
        trace_id: TraceId,
    ) -> AndromedaResult<Result<AuthorizedProcedureDispatch, AuthorizationOutcome>> {
        self.validate_dispatch_preconditions()?;
        authorizer.authorize_procedure_dispatch(
            trace_id,
            self.plane,
            self.certificate_identity.fingerprint().as_str(),
        )
    }

    /// Map a QUIC stream ID to an InvocationId.
    ///
    /// This is a deterministic, stable mapping used to correlate QUIC streams
    /// to invocation execution traces. The mapping is:
    ///
    /// ```text
    /// InvocationId = stream_id (cast to u64)
    /// ```
    ///
    /// This ensures that:
    /// - Each QUIC stream corresponds to exactly one invocation.
    /// - The correlation is bidirectional: stream_id ↔ InvocationId.
    /// - Frame decode → invocation context carries the same InvocationId.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use andromeda_exec::ExecutorDispatchBridge;
    /// # use andromeda_core::InvocationId;
    /// # use andromeda_core::{CertificateIdentity, SurfaceScope};
    /// # let mut conn = andromeda_quic::Connection::new(andromeda_quic::SurfacePlane::Application);
    /// # let identity = CertificateIdentity::new("a".repeat(64), "svc-001", SurfaceScope::Application).unwrap();
    /// # conn.set_certificate_identity(identity).unwrap();
    /// # let bridge = ExecutorDispatchBridge::new(&conn)?;
    ///
    /// let stream_id = 5u64;
    /// let invocation_id = bridge.map_stream_to_invocation_id(stream_id);
    /// assert_eq!(invocation_id, InvocationId::new(stream_id));
    /// # Ok::<(), andromeda_core::AndromedaError>(())
    /// ```
    pub fn map_stream_to_invocation_id(&self, stream_id: u64) -> InvocationId {
        InvocationId::new(stream_id)
    }

    /// Validate that the bridge is in a state suitable for dispatch.
    ///
    /// Returns `Ok(())` if the connection is active and the certificate identity is present.
    /// Returns `Err` with a descriptive error if either condition fails.
    ///
    /// This is a pre-flight check; callers should invoke this before attempting
    /// to authorize or dispatch invocations.
    pub fn validate_dispatch_preconditions(&self) -> AndromedaResult<()> {
        use andromeda_quic::LifecycleState;

        if self.connection.state() != LifecycleState::Active {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "cannot dispatch invocation: connection is in {:?} state, not Active",
                    self.connection.state()
                ),
            ));
        }

        if self.connection.certificate_identity().is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "cannot dispatch invocation: no certificate identity bound to connection",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::SurfaceScope;
    use andromeda_quic::{
        FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader, FrameType, LifecycleState,
    };

    fn hello_frame(session_id: u64) -> FrameBytes {
        FrameBytes {
            header: FrameHeader {
                frame_type: FrameType::Hello,
                request_id: andromeda_core::RequestId::new(1),
                session_id: andromeda_core::SessionId::new(session_id),
                tx_id: None,
                payload_length: 0,
                flags: 0,
                header_crc: FRAME_HEADER_CRC_UNCHECKED,
            },
            payload: Vec::new(),
        }
    }

    fn auth_frame(session_id: u64) -> FrameBytes {
        FrameBytes {
            header: FrameHeader {
                frame_type: FrameType::Auth,
                request_id: andromeda_core::RequestId::new(1),
                session_id: andromeda_core::SessionId::new(session_id),
                tx_id: None,
                payload_length: 0,
                flags: 0,
                header_crc: FRAME_HEADER_CRC_UNCHECKED,
            },
            payload: Vec::new(),
        }
    }

    fn setup_active_application_connection() -> Connection {
        let mut conn = Connection::new(SurfacePlane::Application);
        let identity = CertificateIdentity::new(
            "a".repeat(64),
            "app-service".to_string(),
            SurfaceScope::Application,
        )
        .unwrap();
        conn.set_certificate_identity(identity).unwrap();
        conn.accept_hello(&hello_frame(100)).unwrap();
        conn.accept_auth(&auth_frame(100)).unwrap();
        assert_eq!(conn.state(), LifecycleState::Active);
        conn
    }

    #[test]
    fn test_bridge_accepts_authorized_invocation() {
        let conn = setup_active_application_connection();
        let bridge = ExecutorDispatchBridge::new(&conn).expect("bridge construction failed");

        assert_eq!(bridge.surface_plane(), SurfacePlane::Application);
        assert_eq!(
            bridge.certificate_identity().fingerprint().as_str(),
            "a".repeat(64),
            "certificate fingerprint mismatch"
        );
        assert_eq!(
            bridge.certificate_identity().subject(),
            "app-service",
            "certificate subject mismatch"
        );
        assert_eq!(
            bridge.certificate_identity().surface_scope(),
            SurfaceScope::Application,
            "certificate scope mismatch"
        );

        let stream_id = 42u64;
        let invocation_id = bridge.map_stream_to_invocation_id(stream_id);
        assert_eq!(
            invocation_id,
            InvocationId::new(42),
            "stream mapping failed"
        );

        bridge
            .validate_dispatch_preconditions()
            .expect("dispatch preconditions validation failed");
    }

    #[test]
    fn test_bridge_rejects_cross_plane_invocation() {
        let mut conn = Connection::new(SurfacePlane::Application);
        let admin_identity = CertificateIdentity::new(
            "f".repeat(64),
            "admin-service".to_string(),
            SurfaceScope::Administration,
        )
        .unwrap();

        let result = conn.set_certificate_identity(admin_identity);
        assert!(result.is_err(), "connection should reject mismatched scope");
        let err = result.unwrap_err();
        assert_eq!(
            err.kind(),
            AndromedaErrorKind::Protocol,
            "error kind should be Protocol"
        );
    }

    #[test]
    fn test_bridge_correlates_stream_id_to_invocation() {
        let conn = setup_active_application_connection();
        let bridge = ExecutorDispatchBridge::new(&conn).expect("bridge construction failed");

        let stream_id = 12345u64;
        let inv_id_1 = bridge.map_stream_to_invocation_id(stream_id);
        let inv_id_2 = bridge.map_stream_to_invocation_id(stream_id);
        assert_eq!(inv_id_1, inv_id_2, "mapping should be deterministic");

        assert_eq!(
            inv_id_1,
            InvocationId::new(stream_id),
            "stream_id should map to InvocationId with same value"
        );

        let stream_id_2 = 54321u64;
        let inv_id_3 = bridge.map_stream_to_invocation_id(stream_id_2);
        assert_ne!(
            inv_id_1, inv_id_3,
            "different stream_ids should map to different invocation_ids"
        );
    }

    #[test]
    fn test_bridge_validates_preconditions() {
        let mut conn = Connection::new(SurfacePlane::Application);
        let identity = CertificateIdentity::new(
            "d".repeat(64),
            "test-service".to_string(),
            SurfaceScope::Application,
        )
        .unwrap();
        conn.set_certificate_identity(identity.clone()).unwrap();

        assert_eq!(conn.state(), LifecycleState::Hello);

        let bridge = ExecutorDispatchBridge::new(&conn).expect("bridge construction succeeded");
        let precond_err = bridge.validate_dispatch_preconditions();
        assert!(
            precond_err.is_err(),
            "preconditions should fail for non-Active connection"
        );
        let registry = andromeda_observe::PrincipalRegistry::new();
        let authorizer = SurfacePlaneAuthorizer::new(&registry);
        let authorization_err = bridge
            .authorize_procedure_dispatch(&authorizer, TraceId::new(44))
            .unwrap_err();
        assert_eq!(authorization_err.kind(), AndromedaErrorKind::Protocol);

        let conn_active = setup_active_application_connection();
        let bridge_active =
            ExecutorDispatchBridge::new(&conn_active).expect("bridge construction succeeded");

        let precond_ok = bridge_active.validate_dispatch_preconditions();
        assert!(
            precond_ok.is_ok(),
            "preconditions should succeed for Active connection with identity"
        );
    }

    #[test]
    fn test_bridge_rejects_missing_certificate_identity() {
        let mut conn = Connection::new(SurfacePlane::Application);
        conn.accept_hello(&hello_frame(500)).unwrap();
        conn.accept_auth(&auth_frame(500)).unwrap();
        assert_eq!(conn.state(), LifecycleState::Active);

        let result = ExecutorDispatchBridge::new(&conn);
        assert!(
            result.is_err(),
            "bridge should reject connection without identity"
        );

        let err = result.unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Security);
    }
}
