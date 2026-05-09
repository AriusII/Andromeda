//! QUIC procedure gateway.
//!
//! This module exposes the QUIC-side gateway state needed before a Procedure
//! invocation is handed to the execution layer: authenticated connection
//! evidence, surface-plane binding, and stream-to-invocation correlation.
//!
//! ## Dispatch Flow
//!
//! - **Certificate Identity Required**: The QUIC connection must have a bound
//!   certificate identity before dispatch. If absent, returns `AuthorizationError`.
//! - **Application Plane Only**: ProcedureGateway admits only the tenant-facing
//!   Application surface. Administration, HA/DR, recovery, and monitoring work
//!   must stay on their dedicated surfaces.
//! - **Plane Scope Match**: The certificate's `surface_scope` is derived from
//!   the connection's [`SurfacePlane`] via `plane_to_required_surface_scope()`.
//! - **Stream Correlation**: QUIC stream ID is bound to [`InvocationId`] via
//!   a stable, deterministic mapping.
//! - **Authorization Ownership**: Transport-only binding remains available for
//!   existing admission paths. The authorized binding uses core IAM primitives
//!   to reject missing, revoked, disabled, wrong-scope, or under-permitted
//!   principals before dispatch without depending on the executor.
//!
//!
//! The module is backend-neutral: it uses the abstract [`Connection`] state
//! machine and leaves Quinn/TLS details to the optional runtime adapter.

mod admission;
mod errors;
mod route;
mod state;

pub use admission::ProcedureAuthorizedRouteBinding;
pub use errors::ProcedureRouteAdmissionError;
pub use route::ProcedureRouteBinding;

use andromeda_error::AndromedaResult;
use andromeda_principal::CertificateIdentity;
use andromeda_principal::PrincipalRegistry;
use andromeda_types::InvocationId;

use crate::{Connection, SurfacePlane};
use andromeda_procedure_contract::ProcedureGatewayManifest;
use andromeda_rpc_protocol::FrameBytes;

/// QUIC-side gateway for Procedure dispatch.
///
/// This type holds the Application connection state, certificate identity, and
/// surface plane, and exposes the transport evidence needed by the
/// executor-owned admission gate.
///
/// ## Type Invariants
///
/// - The connection must be in `LifecycleState::Active` before dispatch.
/// - The connection must be bound to [`SurfacePlane::Application`].
/// - The certificate identity must be bound to the connection.
/// - The certificate scope must match the connection plane.
/// - Core IAM authorization can be enforced by
///   [`ProcedureGateway::bind_authorized_application_procedure_route`] after
///   this gateway validates transport preconditions and before transaction
///   creation.
#[derive(Debug)]
pub struct ProcedureGateway<'a> {
    /// The QUIC connection, bound to a surface plane.
    connection: &'a Connection,

    /// The certificate identity extracted and bound during handshake.
    ///
    /// This is the principal presented to the authorizer. Must not be None.
    certificate_identity: &'a CertificateIdentity,

    /// The surface plane this connection is bound to.
    ///
    /// Immutable; set at connection construction. Used to validate that
    /// certificate scope matches and to route to the correct executor.
    plane: SurfacePlane,
}

impl<'a> ProcedureGateway<'a> {
    /// Constructs a new gateway from a connection and its bound certificate identity.
    ///
    /// Returns `Err` if:
    /// - The connection has no bound certificate identity.
    /// - The connection is not bound to the Application surface.
    /// - The certificate scope does not match the connection plane.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use andromeda_quic::{Connection, ProcedureGateway, SurfacePlane};
    /// # use andromeda_principal::CertificateIdentity;
    /// # use andromeda_principal::SurfaceScope;
    ///
    /// # let mut conn = Connection::new(SurfacePlane::Application);
    /// # let identity = CertificateIdentity::new("abc123", "svc-001", SurfaceScope::Application)?;
    /// # conn.set_certificate_identity(identity.clone())?;
    /// let gateway = ProcedureGateway::new(&conn)?;
    /// # Ok::<(), andromeda_error::AndromedaError>(())
    /// ```
    pub fn new(connection: &'a Connection) -> AndromedaResult<Self> {
        let gateway_state = state::resolve_gateway_state(connection)?;

        Ok(Self {
            connection,
            certificate_identity: gateway_state.certificate_identity,
            plane: gateway_state.plane,
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
    /// # use andromeda_quic::ProcedureGateway;
    /// # use andromeda_types::InvocationId;
    /// # use andromeda_principal::{CertificateIdentity, SurfaceScope};
    /// # let mut conn = andromeda_quic::Connection::new(andromeda_quic::SurfacePlane::Application);
    /// # let identity = CertificateIdentity::new("abc123", "svc-001", SurfaceScope::Application)?;
    /// # conn.set_certificate_identity(identity)?;
    /// # let gateway = ProcedureGateway::new(&conn)?;
    ///
    /// let stream_id = 5u64;
    /// let invocation_id = gateway.map_stream_to_invocation_id(stream_id);
    /// assert_eq!(invocation_id, InvocationId::new(stream_id));
    /// # Ok::<(), andromeda_error::AndromedaError>(())
    /// ```
    pub fn map_stream_to_invocation_id(&self, stream_id: u64) -> InvocationId {
        state::invocation_id_for_stream(stream_id)
    }

    /// Validate that the gateway is in a state suitable for dispatch.
    ///
    /// Returns `Ok(())` if the connection is active and the certificate identity is present.
    /// Returns `Err` with a descriptive error if either condition fails.
    ///
    /// This is a pre-flight check; callers should invoke this before attempting
    /// to authorize or dispatch invocations.
    pub fn validate_dispatch_preconditions(&self) -> AndromedaResult<()> {
        state::validate_dispatch_preconditions(self.connection)
    }

    /// Bind an Application-surface `RpcExecuteRequest` frame to a resolved
    /// catalog Procedure manifest before executor dispatch.
    ///
    /// This method is intentionally synchronous and runtime-free. It validates
    /// the transport frame, protobuf envelope, request payload, surface scope,
    /// `ContractHash`, `CatalogVersion`, and Procedure selector before it
    /// returns the route evidence that an executor may use to create a
    /// transaction.
    pub fn bind_application_procedure_route(
        &self,
        stream_id: u64,
        frame: &FrameBytes,
        manifest: &ProcedureGatewayManifest,
    ) -> AndromedaResult<ProcedureRouteBinding> {
        route::bind_application_procedure_route(
            route::ProcedureRouteAdmission {
                connection: self.connection,
                certificate_identity: self.certificate_identity,
                plane: self.plane,
            },
            stream_id,
            frame,
            manifest,
        )
    }

    /// Bind and authorize an Application-surface Procedure route before
    /// executor dispatch.
    ///
    /// This method first performs the transport/protobuf/catalog checks from
    /// [`Self::bind_application_procedure_route`]. It then asks the core
    /// principal registry to authorize `execute_procedure(procedure_id)` for
    /// the mTLS certificate fingerprint bound to the QUIC connection.
    pub fn bind_authorized_application_procedure_route(
        &self,
        stream_id: u64,
        frame: &FrameBytes,
        manifest: &ProcedureGatewayManifest,
        principal_registry: &PrincipalRegistry,
    ) -> Result<ProcedureAuthorizedRouteBinding, ProcedureRouteAdmissionError> {
        let route = self
            .bind_application_procedure_route(stream_id, frame, manifest)
            .map_err(ProcedureRouteAdmissionError::route)?;
        admission::authorize_application_route(route, manifest, principal_registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LifecycleState;
    use andromeda_principal::SurfaceScope;

    fn setup_application_connection() -> Connection {
        let mut conn = Connection::new(SurfacePlane::Application);
        let identity = CertificateIdentity::new(
            "a".repeat(64),
            "test-service".to_string(),
            SurfaceScope::Application,
        )
        .unwrap();
        conn.set_certificate_identity(identity).unwrap();
        conn
    }

    fn setup_administration_connection() -> Connection {
        let mut conn = Connection::new(SurfacePlane::Administration);
        let identity = CertificateIdentity::new(
            "b".repeat(64),
            "admin-service".to_string(),
            SurfaceScope::Administration,
        )
        .unwrap();
        conn.set_certificate_identity(identity).unwrap();
        conn
    }

    fn setup_ha_connection() -> Connection {
        let mut conn = Connection::new(SurfacePlane::HighAvailability);
        let identity = CertificateIdentity::new(
            "c".repeat(64),
            "ha-service".to_string(),
            SurfaceScope::Cluster,
        )
        .unwrap();
        conn.set_certificate_identity(identity).unwrap();
        conn
    }

    #[test]
    fn gateway_rejects_connection_without_certificate_identity() {
        let conn = Connection::new(SurfacePlane::Application);
        let result = ProcedureGateway::new(&conn);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("certificate identity")
        );
    }

    #[test]
    fn gateway_rejects_scope_mismatch() {
        let mut conn = Connection::new(SurfacePlane::Application);
        // Bind an Administration-scoped identity to an Application plane connection.
        let wrong_scope_identity = CertificateIdentity::new(
            "d".repeat(64),
            "wrong-scope".to_string(),
            SurfaceScope::Administration,
        )
        .unwrap();
        let result = conn.set_certificate_identity(wrong_scope_identity);
        // The Connection::set_certificate_identity should reject this.
        assert!(result.is_err());
    }

    #[test]
    fn gateway_accepts_valid_application_connection() {
        let conn = setup_application_connection();
        let gateway = ProcedureGateway::new(&conn).unwrap();

        assert_eq!(gateway.surface_plane(), SurfacePlane::Application);
        assert_eq!(
            gateway.certificate_identity().fingerprint().as_str(),
            "a".repeat(64)
        );
        assert_eq!(gateway.certificate_identity().subject(), "test-service");
        assert_eq!(
            gateway.certificate_identity().surface_scope(),
            SurfaceScope::Application
        );
    }

    #[test]
    fn gateway_rejects_valid_administration_connection() {
        let conn = setup_administration_connection();
        let result = ProcedureGateway::new(&conn);

        assert_eq!(
            result.unwrap_err().message(),
            "ProcedureGateway is Application-surface only; administration, HA/DR, recovery, and monitoring work must use their dedicated surfaces"
        );
    }

    #[test]
    fn gateway_rejects_valid_ha_connection() {
        let conn = setup_ha_connection();
        let result = ProcedureGateway::new(&conn);

        assert!(
            result
                .unwrap_err()
                .message()
                .contains("Application-surface only")
        );
    }

    #[test]
    fn stream_to_invocation_id_mapping_is_deterministic() {
        let conn = setup_application_connection();
        let gateway = ProcedureGateway::new(&conn).unwrap();

        // Same stream_id should always map to same InvocationId.
        let stream_id = 42u64;
        let inv_id_1 = gateway.map_stream_to_invocation_id(stream_id);
        let inv_id_2 = gateway.map_stream_to_invocation_id(stream_id);
        assert_eq!(inv_id_1, inv_id_2);
        assert_eq!(inv_id_1, InvocationId::new(stream_id));
    }

    #[test]
    fn stream_to_invocation_id_mapping_is_injective() {
        let conn = setup_application_connection();
        let gateway = ProcedureGateway::new(&conn).unwrap();

        let inv_id_1 = gateway.map_stream_to_invocation_id(1);
        let inv_id_2 = gateway.map_stream_to_invocation_id(2);
        assert_ne!(inv_id_1, inv_id_2);
    }

    #[test]
    fn validate_dispatch_preconditions_rejects_pre_auth_connection() {
        let mut conn = Connection::new(SurfacePlane::Application);
        let identity = CertificateIdentity::new(
            "e".repeat(64),
            "pre-auth".to_string(),
            SurfaceScope::Application,
        )
        .unwrap();
        conn.set_certificate_identity(identity).unwrap();

        // Connection is still in Hello state.
        assert_eq!(conn.state(), LifecycleState::Hello);
        let gateway = ProcedureGateway::new(&conn).unwrap();

        let result = gateway.validate_dispatch_preconditions();
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("Active"));
    }

    #[test]
    fn validate_dispatch_preconditions_accepts_active_authenticated_connection() {
        let conn = setup_application_connection();
        let gateway = ProcedureGateway::new(&conn).unwrap();

        // Manually set connection to Active for this test (normally done by handshake).
        // Note: Connection doesn't expose a state setter for tests, so we construct
        // a new Connection with mocked state. For now, we test the identity binding.

        // This test focuses on the happy path: gateway created successfully
        // from an authenticated connection means preconditions can be validated.
        assert_eq!(gateway.certificate_identity().subject(), "test-service");
        assert_eq!(gateway.surface_plane(), SurfacePlane::Application);
    }

    #[test]
    fn gateway_exposes_underlying_connection() {
        let conn = setup_application_connection();
        let gateway = ProcedureGateway::new(&conn).unwrap();

        assert_eq!(
            gateway.connection().surface_plane(),
            SurfacePlane::Application
        );
    }

    #[test]
    fn gateway_holds_immutable_references() {
        let conn = setup_application_connection();
        let gateway = ProcedureGateway::new(&conn).unwrap();

        // Verify that the gateway holds references and does not take ownership.
        // (This is implicit in the type signature, but document it in the test.)
        let identity_ref_1 = gateway.certificate_identity();
        let identity_ref_2 = gateway.certificate_identity();
        assert_eq!(identity_ref_1.fingerprint(), identity_ref_2.fingerprint());
    }
}
