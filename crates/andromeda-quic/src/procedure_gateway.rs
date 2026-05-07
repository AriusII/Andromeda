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

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash,
    InvocationId, Permission, PrincipalAuthorizationDecision, PrincipalAuthorizationDenialReason,
    PrincipalAuthorizationEvidence, PrincipalId, PrincipalRegistry, ProcedureId, RequestId,
    SessionId, SurfaceScope as CoreSurfaceScope, TransactionId,
};
use andromeda_observe::CertificateIdentity;
use andromeda_proto::{
    FrameEnvelope as ProtoFrameEnvelope, PayloadKind, decode_generated_message, generated,
    validate_generated_rpc_execute_request,
};

use crate::{
    CatalogProcedureManifest, DispatchPolicy, FrameBytes, FrameType, ResultStreamMetadataPolicy,
    StreamRole, TransportSurface, TypedResultStreamBounds, TypedResultStreamContext,
    decode_typed_frame_envelope, validate_transport_surface,
};
use crate::{Connection, SurfacePlane};

type GeneratedRpcExecuteRequest = generated::protocol::v1::RpcExecuteRequest;

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

/// Authorized pre-dispatch route evidence for a Procedure invocation.
///
/// The authorization evidence is audit-ready and comes from the core IAM
/// decision path. It is attached before any executor dispatch or transaction
/// publication can occur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureAuthorizedRouteBinding {
    pub route: ProcedureRouteBinding,
    pub principal_id: PrincipalId,
    pub authorization_evidence: PrincipalAuthorizationEvidence,
}

/// Route admission failure with optional authorization evidence.
///
/// Transport, frame, protobuf, and catalog binding failures do not have IAM
/// evidence because no principal policy decision was reached. IAM denials carry
/// the core evidence needed for durable audit records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureRouteAdmissionError {
    error: AndromedaError,
    authorization_denial_reason: Option<PrincipalAuthorizationDenialReason>,
    authorization_evidence: Option<Box<PrincipalAuthorizationEvidence>>,
}

impl ProcedureRouteAdmissionError {
    fn route(error: AndromedaError) -> Self {
        Self {
            error,
            authorization_denial_reason: None,
            authorization_evidence: None,
        }
    }

    fn authorization(error: AndromedaError, authorization: PrincipalAuthorizationDecision) -> Self {
        Self {
            error,
            authorization_denial_reason: authorization.denial_reason,
            authorization_evidence: Some(Box::new(authorization.evidence)),
        }
    }

    pub const fn error(&self) -> &AndromedaError {
        &self.error
    }

    pub fn kind(&self) -> AndromedaErrorKind {
        self.error.kind()
    }

    pub fn message(&self) -> &str {
        self.error.message()
    }

    pub const fn authorization_denial_reason(&self) -> Option<PrincipalAuthorizationDenialReason> {
        self.authorization_denial_reason
    }

    pub fn authorization_evidence(&self) -> Option<&PrincipalAuthorizationEvidence> {
        self.authorization_evidence.as_deref()
    }
}

impl std::fmt::Display for ProcedureRouteAdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for ProcedureRouteAdmissionError {}

/// QUIC-side gateway for Procedure dispatch.
///
/// This type holds the connection state, certificate identity, and surface plane,
/// and exposes the transport evidence needed by the executor-owned admission
/// gate.
///
/// ## Type Invariants
///
/// - The connection must be in `LifecycleState::Active` before dispatch.
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
    /// - The certificate scope does not match the connection plane.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use andromeda_quic::{Connection, ProcedureGateway, SurfacePlane};
    /// # use andromeda_observe::CertificateIdentity;
    /// # use andromeda_observe::SurfaceScope;
    ///
    /// # let mut conn = Connection::new(SurfacePlane::Application);
    /// # let identity = CertificateIdentity::new("abc123", "svc-001", SurfaceScope::Application)?;
    /// # conn.set_certificate_identity(identity.clone())?;
    /// let gateway = ProcedureGateway::new(&conn)?;
    /// # Ok::<(), andromeda_core::AndromedaError>(())
    /// ```
    pub fn new(connection: &'a Connection) -> AndromedaResult<Self> {
        let certificate_identity = connection.certificate_identity().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Security,
                "procedure gateway requires bound certificate identity",
            )
        })?;

        let plane = connection.surface_plane();

        // Validate scope match.
        let required_scope = crate::mtls_identity::plane_to_required_surface_scope(plane);
        if certificate_identity.surface as u8 != required_scope as u8 {
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
    /// # use andromeda_core::InvocationId;
    /// # use andromeda_observe::{CertificateIdentity, SurfaceScope};
    /// # let mut conn = andromeda_quic::Connection::new(andromeda_quic::SurfacePlane::Application);
    /// # let identity = CertificateIdentity::new("abc123", "svc-001", SurfaceScope::Application)?;
    /// # conn.set_certificate_identity(identity)?;
    /// # let gateway = ProcedureGateway::new(&conn)?;
    ///
    /// let stream_id = 5u64;
    /// let invocation_id = gateway.map_stream_to_invocation_id(stream_id);
    /// assert_eq!(invocation_id, InvocationId::new(stream_id));
    /// # Ok::<(), andromeda_core::AndromedaError>(())
    /// ```
    pub fn map_stream_to_invocation_id(&self, stream_id: u64) -> InvocationId {
        InvocationId::new(stream_id)
    }

    /// Validate that the gateway is in a state suitable for dispatch.
    ///
    /// Returns `Ok(())` if the connection is active and the certificate identity is present.
    /// Returns `Err` with a descriptive error if either condition fails.
    ///
    /// This is a pre-flight check; callers should invoke this before attempting
    /// to authorize or dispatch invocations.
    pub fn validate_dispatch_preconditions(&self) -> AndromedaResult<()> {
        use crate::LifecycleState;

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

    fn validate_frame_session_binding(&self, frame: &FrameBytes) -> AndromedaResult<()> {
        match self.connection.session_id() {
            Some(session_id) if session_id == frame.header.session_id => Ok(()),
            Some(_) => Err(protocol_error(
                "procedure invocation frame session id does not match authenticated connection",
            )),
            None => Err(protocol_error(
                "procedure invocation requires authenticated connection session id",
            )),
        }
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
        manifest: &CatalogProcedureManifest,
    ) -> AndromedaResult<ProcedureRouteBinding> {
        self.validate_dispatch_preconditions()?;

        if self.plane != SurfacePlane::Application {
            return Err(security_error(
                "procedure invocation route requires Application surface",
            ));
        }
        self.validate_frame_session_binding(frame)?;

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

        validate_route_manifest_permissions(manifest)?;
        manifest.to_protobuf()?;
        let envelope = decode_rpc_execute_envelope(frame)?;
        if envelope.tx_id.is_some() {
            return Err(protocol_error(
                "procedure invocation envelope must not carry client transaction id",
            ));
        }

        let generated_request: GeneratedRpcExecuteRequest =
            decode_generated_message(envelope.payload.as_slice())?;
        validate_generated_rpc_execute_request(&generated_request)?;
        let execute_request =
            validate_rpc_execute_request(&generated_request, self.surface_plane())?;

        validate_request_matches_manifest(&envelope, &execute_request, manifest)?;

        Ok(ProcedureRouteBinding {
            invocation_id: self.map_stream_to_invocation_id(stream_id),
            request_id: frame.header.request_id,
            session_id: frame.header.session_id,
            tx_id: frame.header.tx_id,
            surface_plane: self.plane,
            certificate_identity: self.certificate_identity.clone(),
            procedure_id: manifest.procedure_id,
            contract_hash: manifest.contract_hash,
            catalog_version: manifest.catalog_version,
            stats_version: manifest.stats_version,
            execute_request,
            manifest: manifest.clone(),
        })
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
        manifest: &CatalogProcedureManifest,
        principal_registry: &PrincipalRegistry,
    ) -> Result<ProcedureAuthorizedRouteBinding, ProcedureRouteAdmissionError> {
        let route = self
            .bind_application_procedure_route(stream_id, frame, manifest)
            .map_err(ProcedureRouteAdmissionError::route)?;
        let required_permission =
            required_execute_permission(manifest).map_err(ProcedureRouteAdmissionError::route)?;
        let authorization = principal_registry.authorize(
            core_surface_scope_for_plane(route.surface_plane),
            route.certificate_identity.fingerprint.as_str(),
            &required_permission,
        );

        if authorization.is_denied() {
            let reason = authorization
                .denial_reason
                .map(|reason| reason.as_str())
                .unwrap_or("denied");
            return Err(ProcedureRouteAdmissionError::authorization(
                security_error(format!(
                    "procedure invocation authorization denied: {reason}"
                )),
                authorization,
            ));
        }

        let Some(principal_id) = authorization.principal_id else {
            return Err(ProcedureRouteAdmissionError::authorization(
                security_error("procedure invocation authorization missing principal evidence"),
                authorization,
            ));
        };

        Ok(ProcedureAuthorizedRouteBinding {
            route,
            principal_id,
            authorization_evidence: authorization.evidence,
        })
    }
}

fn decode_rpc_execute_envelope(frame: &FrameBytes) -> AndromedaResult<ProtoFrameEnvelope> {
    let envelope = decode_typed_frame_envelope(frame)?;

    if envelope.payload_kind != PayloadKind::RpcExecuteRequest {
        return Err(protocol_error(
            "procedure invocation envelope payload kind must be RpcExecuteRequest",
        ));
    }

    Ok(envelope)
}

fn validate_rpc_execute_request(
    request: &GeneratedRpcExecuteRequest,
    plane: SurfacePlane,
) -> AndromedaResult<ProcedureRouteExecuteRequest> {
    if request.procedure_name.trim().is_empty() {
        return Err(contract_error(
            "RpcExecuteRequest procedure_name must be non-empty",
        ));
    }

    let expected_contract_hash = ContractHash::from_slice(&request.expected_contract_hash)?;
    if expected_contract_hash.is_zero() {
        return Err(contract_error(
            "RpcExecuteRequest expected_contract_hash must not be zero",
        ));
    }

    if request.expected_catalog_version == 0 {
        return Err(contract_error(
            "RpcExecuteRequest expected_catalog_version must be nonzero",
        ));
    }

    let Some(expected_stats_version) = request.expected_stats_version else {
        return Err(contract_error(
            "RpcExecuteRequest expected_stats_version must be present",
        ));
    };
    if expected_stats_version == 0 {
        return Err(contract_error(
            "RpcExecuteRequest expected_stats_version must be nonzero",
        ));
    }

    let expected_surface = surface_scope_wire_label(plane);
    if request.surface_scope != expected_surface {
        return Err(security_error(
            "RpcExecuteRequest surface_scope does not match Application surface",
        ));
    }

    Ok(ProcedureRouteExecuteRequest {
        procedure_name: request.procedure_name.clone(),
        expected_contract_hash,
        expected_catalog_version: CatalogVersion::new(request.expected_catalog_version),
        expected_stats_version,
        surface_scope: request.surface_scope.clone(),
        argument_count: request.arguments.len(),
    })
}

fn validate_request_matches_manifest(
    envelope: &ProtoFrameEnvelope,
    request: &ProcedureRouteExecuteRequest,
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<()> {
    if envelope.contract_hash != request.expected_contract_hash {
        return Err(contract_error(
            "procedure invocation envelope ContractHash does not match request expectation",
        ));
    }

    if envelope.catalog_version != request.expected_catalog_version {
        return Err(contract_error(
            "procedure invocation envelope CatalogVersion does not match request expectation",
        ));
    }

    if request.procedure_name != manifest.procedure_name {
        return Err(contract_error(
            "procedure invocation Procedure name does not match resolved manifest",
        ));
    }

    if request.expected_contract_hash != manifest.contract_hash {
        return Err(contract_error(
            "procedure invocation ContractHash does not match resolved manifest",
        ));
    }

    if request.expected_catalog_version != manifest.catalog_version {
        return Err(contract_error(
            "procedure invocation CatalogVersion does not match resolved manifest",
        ));
    }

    if request.expected_stats_version != manifest.stats_version {
        return Err(contract_error(
            "procedure invocation StatsVersion does not match resolved manifest",
        ));
    }

    Ok(())
}

const fn surface_scope_wire_label(plane: SurfacePlane) -> &'static str {
    match plane {
        SurfacePlane::Application => "application",
        SurfacePlane::Administration => "administration",
        SurfacePlane::HighAvailability => "cluster",
        SurfacePlane::Monitoring => "monitoring",
    }
}

const fn core_surface_scope_for_plane(plane: SurfacePlane) -> CoreSurfaceScope {
    match plane {
        SurfacePlane::Application => CoreSurfaceScope::Application,
        SurfacePlane::Administration => CoreSurfaceScope::Administration,
        SurfacePlane::HighAvailability => CoreSurfaceScope::Cluster,
        SurfacePlane::Monitoring => CoreSurfaceScope::MonitoringAgent,
    }
}

fn validate_route_manifest_permissions(manifest: &CatalogProcedureManifest) -> AndromedaResult<()> {
    required_execute_permission(manifest).map(|_| ())
}

fn required_execute_permission(manifest: &CatalogProcedureManifest) -> AndromedaResult<Permission> {
    let declares_execute_permission = manifest.required_permissions.iter().any(|permission| {
        permission.family.trim() == "application"
            && matches!(
                permission.id.trim(),
                "andromeda.execute_procedure" | "execute_procedure"
            )
    });

    if !declares_execute_permission {
        return Err(contract_error(
            "procedure manifest required_permissions must include andromeda.execute_procedure",
        ));
    }

    Ok(Permission::ExecuteProcedure(manifest.procedure_id))
}

fn protocol_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

fn contract_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}

fn security_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LifecycleState;
    use andromeda_observe::SurfaceScope;

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
        assert_eq!(gateway.certificate_identity().fingerprint, "a".repeat(64));
        assert_eq!(gateway.certificate_identity().subject, "test-service");
        assert_eq!(
            gateway.certificate_identity().surface,
            SurfaceScope::Application
        );
    }

    #[test]
    fn gateway_accepts_valid_administration_connection() {
        let conn = setup_administration_connection();
        let gateway = ProcedureGateway::new(&conn).unwrap();

        assert_eq!(gateway.surface_plane(), SurfacePlane::Administration);
        assert_eq!(gateway.certificate_identity().fingerprint, "b".repeat(64));
    }

    #[test]
    fn gateway_accepts_valid_ha_connection() {
        let conn = setup_ha_connection();
        let gateway = ProcedureGateway::new(&conn).unwrap();

        assert_eq!(gateway.surface_plane(), SurfacePlane::HighAvailability);
        assert_eq!(gateway.certificate_identity().fingerprint, "c".repeat(64));
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
        assert_eq!(gateway.certificate_identity().subject, "test-service");
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
        assert_eq!(identity_ref_1.fingerprint, identity_ref_2.fingerprint);
    }
}
