//! QUIC route contract for catalog procedure manifest resolution.
//!
//! Catalog manifest resolution is transported as a reliable command stream:
//! `ContractRequest` frames carry a protobuf `FrameEnvelope` whose payload is a
//! generated `CatalogProcedureManifestResolutionRequest`; route responses use
//! `ContractResponse` with a generated
//! `CatalogProcedureManifestResolutionResponse`.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, ProcedureId,
    RequestId, SessionId, TransactionId,
};
use andromeda_observe::{CertificateIdentity, SurfaceScope};
use andromeda_proto::{
    FrameEnvelope as ProtoFrameEnvelope, PayloadKind, ProtocolVersion, decode_generated_message,
    encode_generated_message, generated, validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response,
};

use crate::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader, FrameType, StreamRole, SurfacePlane,
    TransportEndpointMetadata, TransportMessage, TransportSurface, validate_transport_surface,
};

pub use generated::contract::v1::{
    CatalogProcedureManifestResolutionRequest, CatalogProcedureManifestResolutionResponse,
    catalog_procedure_manifest_resolution_response::Status as CatalogManifestResolutionStatus,
};

type GeneratedCatalogManifestResolutionRequest =
    generated::contract::v1::CatalogProcedureManifestResolutionRequest;
type GeneratedCatalogManifestResolutionResponse =
    generated::contract::v1::CatalogProcedureManifestResolutionResponse;
type GeneratedCatalogManifestSelector =
    generated::contract::v1::catalog_procedure_manifest_resolution_request::Selector;
type GeneratedProcedureManifest = generated::contract::v1::ProcedureManifest;
type GeneratedProtocolLayout = generated::contract::v1::ProtocolLayout;
type GeneratedRequiredPermission = generated::contract::v1::RequiredPermission;
type GeneratedResultStreamDescriptor = generated::contract::v1::ResultStreamDescriptor;
type GeneratedColumnDescriptor = generated::contract::v1::ColumnDescriptor;
type GeneratedFrameEnvelope = generated::protocol::v1::FrameEnvelope;
type GeneratedProtocolVersion = generated::protocol::v1::ProtocolVersion;

/// QUIC-local domain selector for catalog manifest resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogManifestSelector {
    ProcedureId(ProcedureId),
    ProcedureName(String),
}

/// Domain request used inside the QUIC route after protobuf boundary decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolutionRequest {
    pub protocol_major: u32,
    pub protocol_minor: u32,
    pub request_id: RequestId,
    pub trace_id: Option<String>,
    pub selector: CatalogManifestSelector,
    pub expected_contract_hash: Option<ContractHash>,
    pub expected_catalog_version: Option<CatalogVersion>,
    pub require_source_generator_ready: bool,
}

impl CatalogManifestResolutionRequest {
    pub fn from_protobuf(
        request: GeneratedCatalogManifestResolutionRequest,
    ) -> AndromedaResult<Self> {
        validate_catalog_procedure_manifest_resolution_request(&request)?;

        let selector = match request.selector {
            Some(GeneratedCatalogManifestSelector::ProcedureId(procedure_id)) => {
                CatalogManifestSelector::ProcedureId(ProcedureId::new(procedure_id))
            }
            Some(GeneratedCatalogManifestSelector::ProcedureName(procedure_name)) => {
                CatalogManifestSelector::ProcedureName(procedure_name)
            }
            None => {
                return Err(protocol_error(
                    "catalog manifest resolution request requires selector",
                ));
            }
        };
        let expected_contract_hash = request
            .expected_contract_hash
            .as_deref()
            .map(ContractHash::from_slice)
            .transpose()?;
        let expected_catalog_version = request.expected_catalog_version.map(CatalogVersion::new);

        Ok(Self {
            protocol_major: request.protocol_major,
            protocol_minor: request.protocol_minor,
            request_id: RequestId::new(request.request_id),
            trace_id: request.trace_id,
            selector,
            expected_contract_hash,
            expected_catalog_version,
            require_source_generator_ready: request.require_source_generator_ready,
        })
    }
}

/// Domain response returned by the catalog manifest runtime before protobuf encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolutionResponse {
    pub protocol_major: u32,
    pub protocol_minor: u32,
    pub request_id: RequestId,
    pub trace_id: Option<String>,
    pub status: CatalogManifestResolutionStatus,
    pub manifest: Option<CatalogProcedureManifest>,
    pub current_catalog_version: Option<CatalogVersion>,
    pub diagnostic_code: Option<String>,
}

impl CatalogManifestResolutionResponse {
    pub fn to_protobuf(&self) -> AndromedaResult<GeneratedCatalogManifestResolutionResponse> {
        if self.status == CatalogManifestResolutionStatus::Unspecified {
            return Err(protocol_error(
                "catalog manifest resolution status must be specified",
            ));
        }

        let manifest = self
            .manifest
            .as_ref()
            .map(CatalogProcedureManifest::to_protobuf)
            .transpose()?;
        let resolved = self.status == CatalogManifestResolutionStatus::Resolved;
        let resolved_contract_hash = if resolved {
            manifest
                .as_ref()
                .map(|manifest| manifest.contract_hash.clone())
        } else {
            None
        };
        let resolved_catalog_version = if resolved {
            manifest.as_ref().map(|manifest| manifest.catalog_version)
        } else {
            None
        };
        let response = GeneratedCatalogManifestResolutionResponse {
            protocol_major: self.protocol_major,
            protocol_minor: self.protocol_minor,
            request_id: self.request_id.get(),
            trace_id: self.trace_id.clone(),
            status: self.status as i32,
            manifest,
            resolved_contract_hash,
            resolved_catalog_version,
            current_catalog_version: self.current_catalog_version.map(CatalogVersion::get),
            diagnostic_code: self.diagnostic_code.clone(),
        };
        validate_catalog_procedure_manifest_resolution_response(&response)?;
        Ok(response)
    }

    pub fn validate_against_request(
        &self,
        request: &CatalogManifestResolutionRequest,
    ) -> AndromedaResult<()> {
        if self.status != CatalogManifestResolutionStatus::Resolved {
            return Ok(());
        }

        let Some(manifest) = self.manifest.as_ref() else {
            return Err(contract_error(
                "resolved catalog manifest response requires manifest evidence",
            ));
        };

        match &request.selector {
            CatalogManifestSelector::ProcedureId(expected) => {
                if manifest.procedure_id != *expected {
                    return Err(contract_error(
                        "resolved catalog manifest procedure id does not match request selector",
                    ));
                }
            }
            CatalogManifestSelector::ProcedureName(expected) => {
                if manifest.procedure_name != *expected {
                    return Err(contract_error(
                        "resolved catalog manifest procedure name does not match request selector",
                    ));
                }
            }
        }

        if let Some(expected_contract_hash) = request.expected_contract_hash
            && manifest.contract_hash != expected_contract_hash
        {
            return Err(contract_error(
                "resolved catalog manifest contract hash does not match request expectation",
            ));
        }

        if let Some(expected_catalog_version) = request.expected_catalog_version
            && manifest.catalog_version != expected_catalog_version
        {
            return Err(contract_error(
                "resolved catalog manifest catalog version does not match request expectation",
            ));
        }

        Ok(())
    }
}

/// Domain Procedure manifest used inside the QUIC catalog route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogProcedureManifest {
    pub procedure_id: ProcedureId,
    pub procedure_name: String,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub protocol_layout: CatalogProcedureProtocolLayout,
    pub result_streams: Vec<CatalogResultStreamDescriptor>,
    pub stats_version: u64,
    pub policy_version: ContractHash,
    pub required_permissions: Vec<CatalogRequiredPermission>,
}

impl CatalogProcedureManifest {
    pub fn to_protobuf(&self) -> AndromedaResult<GeneratedProcedureManifest> {
        let manifest = GeneratedProcedureManifest {
            procedure_id: self.procedure_id.get(),
            procedure_name: self.procedure_name.clone(),
            contract_hash: self.contract_hash.as_bytes().to_vec(),
            catalog_version: self.catalog_version.get(),
            protocol_layout: Some(self.protocol_layout.to_protobuf()),
            result_streams: self
                .result_streams
                .iter()
                .map(CatalogResultStreamDescriptor::to_protobuf)
                .collect(),
            policy_version: self.policy_version.as_bytes().to_vec(),
            required_permissions: self
                .required_permissions
                .iter()
                .map(CatalogRequiredPermission::to_protobuf)
                .collect(),
            stats_version: Some(self.stats_version),
        };
        validate_catalog_procedure_manifest_resolution_response(
            &GeneratedCatalogManifestResolutionResponse {
                protocol_major: 1,
                protocol_minor: 0,
                request_id: 1,
                trace_id: None,
                status: CatalogManifestResolutionStatus::Resolved as i32,
                manifest: Some(manifest.clone()),
                resolved_contract_hash: Some(manifest.contract_hash.clone()),
                resolved_catalog_version: Some(manifest.catalog_version),
                current_catalog_version: Some(manifest.catalog_version),
                diagnostic_code: None,
            },
        )?;
        Ok(manifest)
    }
}

impl TryFrom<GeneratedProcedureManifest> for CatalogProcedureManifest {
    type Error = AndromedaError;

    fn try_from(manifest: GeneratedProcedureManifest) -> AndromedaResult<Self> {
        validate_catalog_procedure_manifest_resolution_response(
            &GeneratedCatalogManifestResolutionResponse {
                protocol_major: 1,
                protocol_minor: 0,
                request_id: 1,
                trace_id: None,
                status: CatalogManifestResolutionStatus::Resolved as i32,
                manifest: Some(manifest.clone()),
                resolved_contract_hash: Some(manifest.contract_hash.clone()),
                resolved_catalog_version: Some(manifest.catalog_version),
                current_catalog_version: Some(manifest.catalog_version),
                diagnostic_code: None,
            },
        )?;

        let Some(protocol_layout) = manifest.protocol_layout else {
            return Err(protocol_error(
                "resolved procedure manifest requires protocol layout",
            ));
        };
        let Some(stats_version) = manifest.stats_version else {
            return Err(protocol_error(
                "resolved procedure manifest requires stats version",
            ));
        };

        Ok(Self {
            procedure_id: ProcedureId::new(manifest.procedure_id),
            procedure_name: manifest.procedure_name,
            contract_hash: ContractHash::from_slice(&manifest.contract_hash)?,
            catalog_version: CatalogVersion::new(manifest.catalog_version),
            protocol_layout: CatalogProcedureProtocolLayout::try_from(protocol_layout)?,
            result_streams: manifest
                .result_streams
                .into_iter()
                .map(CatalogResultStreamDescriptor::from)
                .collect(),
            stats_version,
            policy_version: ContractHash::from_slice(&manifest.policy_version)?,
            required_permissions: manifest
                .required_permissions
                .into_iter()
                .map(CatalogRequiredPermission::from)
                .collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogProcedureProtocolLayout {
    pub descriptor_set_hash: ContractHash,
    pub frame_envelope_hash: ContractHash,
    pub protocol_package: String,
    pub contract_package: String,
}

impl CatalogProcedureProtocolLayout {
    fn to_protobuf(&self) -> GeneratedProtocolLayout {
        GeneratedProtocolLayout {
            descriptor_set_hash: self.descriptor_set_hash.as_bytes().to_vec(),
            frame_envelope_hash: self.frame_envelope_hash.as_bytes().to_vec(),
            protocol_package: self.protocol_package.clone(),
            contract_package: self.contract_package.clone(),
        }
    }
}

impl TryFrom<GeneratedProtocolLayout> for CatalogProcedureProtocolLayout {
    type Error = AndromedaError;

    fn try_from(layout: GeneratedProtocolLayout) -> AndromedaResult<Self> {
        Ok(Self {
            descriptor_set_hash: ContractHash::from_slice(&layout.descriptor_set_hash)?,
            frame_envelope_hash: ContractHash::from_slice(&layout.frame_envelope_hash)?,
            protocol_package: layout.protocol_package,
            contract_package: layout.contract_package,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRequiredPermission {
    pub id: String,
    pub family: String,
}

impl CatalogRequiredPermission {
    fn to_protobuf(&self) -> GeneratedRequiredPermission {
        GeneratedRequiredPermission {
            id: self.id.clone(),
            family: self.family.clone(),
        }
    }
}

impl From<GeneratedRequiredPermission> for CatalogRequiredPermission {
    fn from(permission: GeneratedRequiredPermission) -> Self {
        Self {
            id: permission.id,
            family: permission.family,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogResultStreamDescriptor {
    pub stream_name: String,
    pub columns: Vec<CatalogColumnDescriptor>,
    pub cardinality: i32,
    pub row_count_requirement: i32,
    pub row_count_exact: Option<u64>,
    pub row_count_max: Option<u64>,
}

impl CatalogResultStreamDescriptor {
    fn to_protobuf(&self) -> GeneratedResultStreamDescriptor {
        GeneratedResultStreamDescriptor {
            stream_name: self.stream_name.clone(),
            columns: self
                .columns
                .iter()
                .map(CatalogColumnDescriptor::to_protobuf)
                .collect(),
            cardinality: self.cardinality,
            row_count_requirement: self.row_count_requirement,
            row_count_exact: self.row_count_exact,
            row_count_max: self.row_count_max,
        }
    }
}

impl From<GeneratedResultStreamDescriptor> for CatalogResultStreamDescriptor {
    fn from(stream: GeneratedResultStreamDescriptor) -> Self {
        Self {
            stream_name: stream.stream_name,
            columns: stream
                .columns
                .into_iter()
                .map(CatalogColumnDescriptor::from)
                .collect(),
            cardinality: stream.cardinality,
            row_count_requirement: stream.row_count_requirement,
            row_count_exact: stream.row_count_exact,
            row_count_max: stream.row_count_max,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogColumnDescriptor {
    pub name: String,
    pub ordinal: u32,
    pub type_name: String,
}

impl CatalogColumnDescriptor {
    fn to_protobuf(&self) -> GeneratedColumnDescriptor {
        GeneratedColumnDescriptor {
            name: self.name.clone(),
            ordinal: self.ordinal,
            type_name: self.type_name.clone(),
        }
    }
}

impl From<GeneratedColumnDescriptor> for CatalogColumnDescriptor {
    fn from(column: GeneratedColumnDescriptor) -> Self {
        Self {
            name: column.name,
            ordinal: column.ordinal,
            type_name: column.type_name,
        }
    }
}

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

        if !metadata
            .surface_plane()
            .permits_family(frame.header.frame_type.frame_family())
        {
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

        let request_envelope = decode_catalog_envelope(frame, PayloadKind::ContractRequest)?;
        let generated_request: GeneratedCatalogManifestResolutionRequest =
            decode_generated_message(request_envelope.payload.as_slice())?;
        validate_catalog_procedure_manifest_resolution_request(&generated_request)?;
        validate_request_context(frame, &request_envelope, generated_request.request_id)?;
        let request = CatalogManifestResolutionRequest::from_protobuf(generated_request)?;

        let context = CatalogManifestResolutionContext {
            request_id: frame.header.request_id,
            session_id: frame.header.session_id,
            tx_id: frame.header.tx_id,
            surface_plane: metadata.surface_plane(),
            certificate_identity: metadata.certificate_identity().cloned(),
            trace_id: request.trace_id.clone(),
        };

        let request_for_validation = request.clone();
        let response = self.runtime.resolve_catalog_manifest(&context, request)?;
        validate_response_context(&context, &response)?;
        response.validate_against_request(&request_for_validation)?;
        let generated_response = response.to_protobuf()?;

        let response_frame =
            encode_catalog_response_frame(&request_envelope, frame.header, &generated_response)?;
        validate_transport_surface(
            &response_frame,
            TransportSurface::ReliableStream(StreamRole::CommandBidirectional),
        )?;

        Ok(response_frame)
    }
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

    if identity.surface != SurfaceScope::Administration {
        return Err(security_error(
            "catalog manifest resolution certificate scope must match Administration surface",
        ));
    }

    Ok(())
}

/// Builds a `ContractRequest` frame carrying a catalog manifest resolution request.
pub fn catalog_manifest_resolution_request_frame(
    request: &CatalogProcedureManifestResolutionRequest,
    session_id: SessionId,
    tx_id: Option<TransactionId>,
    contract_hash: ContractHash,
    catalog_version: CatalogVersion,
) -> AndromedaResult<FrameBytes> {
    validate_catalog_procedure_manifest_resolution_request(request)?;

    let request_id = RequestId::new(request.request_id);
    let request_payload = encode_generated_message(request);
    let envelope = ProtoFrameEnvelope {
        protocol_version: ProtocolVersion {
            major: request.protocol_major,
            minor: request.protocol_minor,
        },
        contract_hash,
        catalog_version,
        request_id,
        session_id,
        tx_id,
        payload_kind: PayloadKind::ContractRequest,
        payload: request_payload,
    };
    envelope.validate()?;

    Ok(frame_with_payload(
        FrameType::ContractRequest,
        request_id,
        session_id,
        tx_id,
        encode_generated_envelope(&envelope),
    ))
}

/// Decodes and validates a catalog manifest resolution request frame.
pub fn decode_catalog_manifest_resolution_request_frame(
    frame: &FrameBytes,
) -> AndromedaResult<CatalogProcedureManifestResolutionRequest> {
    validate_transport_surface(
        frame,
        TransportSurface::ReliableStream(StreamRole::CommandBidirectional),
    )?;

    if frame.header.frame_type != FrameType::ContractRequest {
        return Err(protocol_error(
            "catalog manifest resolution request decoder requires ContractRequest frame",
        ));
    }

    let envelope = decode_catalog_envelope(frame, PayloadKind::ContractRequest)?;
    let request: CatalogProcedureManifestResolutionRequest =
        decode_generated_message(envelope.payload.as_slice())?;
    validate_catalog_procedure_manifest_resolution_request(&request)?;
    validate_request_context(frame, &envelope, request.request_id)?;

    Ok(request)
}

/// Decodes and validates a catalog manifest resolution response frame.
pub fn decode_catalog_manifest_resolution_response_frame(
    frame: &FrameBytes,
) -> AndromedaResult<CatalogProcedureManifestResolutionResponse> {
    validate_transport_surface(
        frame,
        TransportSurface::ReliableStream(StreamRole::CommandBidirectional),
    )?;

    if frame.header.frame_type != FrameType::ContractResponse {
        return Err(protocol_error(
            "catalog manifest resolution response decoder requires ContractResponse frame",
        ));
    }

    let envelope = decode_catalog_envelope(frame, PayloadKind::ContractResponse)?;
    let response: CatalogProcedureManifestResolutionResponse =
        decode_generated_message(envelope.payload.as_slice())?;
    validate_catalog_procedure_manifest_resolution_response(&response)?;
    validate_request_context(frame, &envelope, response.request_id)?;

    Ok(response)
}

fn encode_catalog_response_frame(
    request_envelope: &ProtoFrameEnvelope,
    request_header: FrameHeader,
    response: &CatalogProcedureManifestResolutionResponse,
) -> AndromedaResult<FrameBytes> {
    let response_payload = encode_generated_message(response);
    let response_catalog_version = response
        .current_catalog_version
        .or(response.resolved_catalog_version)
        .unwrap_or_else(|| request_envelope.catalog_version.get());
    let envelope = ProtoFrameEnvelope {
        protocol_version: request_envelope.protocol_version,
        contract_hash: request_envelope.contract_hash,
        catalog_version: CatalogVersion::new(response_catalog_version),
        request_id: request_envelope.request_id,
        session_id: request_envelope.session_id,
        tx_id: request_envelope.tx_id,
        payload_kind: PayloadKind::ContractResponse,
        payload: response_payload,
    };
    envelope.validate()?;

    Ok(frame_with_payload(
        FrameType::ContractResponse,
        request_header.request_id,
        request_header.session_id,
        request_header.tx_id,
        encode_generated_envelope(&envelope),
    ))
}

fn decode_catalog_envelope(
    frame: &FrameBytes,
    expected_payload_kind: PayloadKind,
) -> AndromedaResult<ProtoFrameEnvelope> {
    let generated_envelope: GeneratedFrameEnvelope = decode_generated_message(&frame.payload)?;
    let payload_kind = PayloadKind::try_from(generated_envelope.payload_kind as u32)?;
    let Some(protocol_version) = generated_envelope.protocol_version else {
        return Err(protocol_error(
            "catalog manifest resolution envelope requires protocol version",
        ));
    };
    let contract_hash =
        if generated_envelope.contract_hash.is_empty() && !payload_kind.requires_contract_hash() {
            ContractHash::zero()
        } else {
            ContractHash::from_slice(&generated_envelope.contract_hash)?
        };

    let envelope = ProtoFrameEnvelope {
        protocol_version: ProtocolVersion {
            major: protocol_version.major,
            minor: protocol_version.minor,
        },
        contract_hash,
        catalog_version: CatalogVersion::new(generated_envelope.catalog_version),
        request_id: RequestId::new(generated_envelope.request_id),
        session_id: SessionId::new(generated_envelope.session_id),
        tx_id: generated_envelope.tx_id.map(TransactionId::new),
        payload_kind,
        payload: generated_envelope.payload,
    };
    envelope.validate()?;

    if envelope.payload_kind != expected_payload_kind {
        return Err(protocol_error(
            "catalog manifest resolution envelope payload kind does not match frame route",
        ));
    }

    if envelope.request_id != frame.header.request_id
        || envelope.session_id != frame.header.session_id
        || envelope.tx_id != frame.header.tx_id
    {
        return Err(protocol_error(
            "catalog manifest resolution envelope context does not match frame header",
        ));
    }

    Ok(envelope)
}

fn validate_request_context(
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

fn validate_response_context(
    context: &CatalogManifestResolutionContext,
    response: &CatalogManifestResolutionResponse,
) -> AndromedaResult<()> {
    if response.request_id != context.request_id {
        return Err(protocol_error(
            "catalog manifest resolution runtime changed response request id",
        ));
    }

    Ok(())
}

fn encode_generated_envelope(envelope: &ProtoFrameEnvelope) -> Vec<u8> {
    encode_generated_message(&GeneratedFrameEnvelope {
        protocol_version: Some(GeneratedProtocolVersion {
            major: envelope.protocol_version.major,
            minor: envelope.protocol_version.minor,
        }),
        contract_hash: envelope.contract_hash.as_bytes().to_vec(),
        catalog_version: envelope.catalog_version.get(),
        request_id: envelope.request_id.get(),
        session_id: envelope.session_id.get(),
        tx_id: envelope.tx_id.map(TransactionId::get),
        payload_kind: envelope.payload_kind.wire_code() as i32,
        payload: envelope.payload.clone(),
    })
}

fn frame_with_payload(
    frame_type: FrameType,
    request_id: RequestId,
    session_id: SessionId,
    tx_id: Option<TransactionId>,
    payload: Vec<u8>,
) -> FrameBytes {
    FrameBytes {
        header: FrameHeader {
            frame_type,
            request_id,
            session_id,
            tx_id,
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    }
}

fn protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

fn contract_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}
