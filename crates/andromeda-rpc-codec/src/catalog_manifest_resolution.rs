use andromeda_catalog_store::CatalogManifestResolutionStatus as StoreCatalogManifestResolutionStatus;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
pub use andromeda_procedure_contract::{
    ProcedureGatewayColumnDescriptor as CatalogColumnDescriptor,
    ProcedureGatewayManifest as CatalogProcedureManifest,
    ProcedureGatewayProtocolLayout as CatalogProcedureProtocolLayout,
    ProcedureGatewayRequiredPermission as CatalogRequiredPermission,
    ProcedureGatewayResultStreamDescriptor as CatalogResultStreamDescriptor,
};
use andromeda_proto::{
    FrameEnvelope as ProtoFrameEnvelope, PayloadKind, ProtocolVersion, decode_generated_message,
    encode_generated_message, generated, project_generated_frame_envelope,
    validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response,
};
use andromeda_rpc_protocol::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader, FrameType, StreamRole,
    validate_single_frame_on_stream,
};
use andromeda_types::{
    CatalogVersion, ContractHash, ProcedureId, RequestId, SessionId, TransactionId,
};

pub use generated::contract::v1::{
    CatalogProcedureManifestResolutionRequest, CatalogProcedureManifestResolutionResponse,
    catalog_procedure_manifest_resolution_response::Status as CatalogManifestResolutionStatus,
};

/// Projects a protocol-free catalog-store runtime status into the generated
/// Protobuf status enum owned by the RPC/proto boundary.
pub const fn catalog_manifest_resolution_status_to_protobuf(
    status: StoreCatalogManifestResolutionStatus,
) -> CatalogManifestResolutionStatus {
    match status {
        StoreCatalogManifestResolutionStatus::Resolved => CatalogManifestResolutionStatus::Resolved,
        StoreCatalogManifestResolutionStatus::NotFound => CatalogManifestResolutionStatus::NotFound,
        StoreCatalogManifestResolutionStatus::CatalogVersionMismatch => {
            CatalogManifestResolutionStatus::CatalogVersionMismatch
        },
        StoreCatalogManifestResolutionStatus::ContractHashMismatch => {
            CatalogManifestResolutionStatus::ContractHashMismatch
        },
        StoreCatalogManifestResolutionStatus::NotSourceGeneratorReady => {
            CatalogManifestResolutionStatus::NotSourceGeneratorReady
        },
        StoreCatalogManifestResolutionStatus::PermissionDenied => {
            CatalogManifestResolutionStatus::PermissionDenied
        },
        StoreCatalogManifestResolutionStatus::Unsupported => {
            CatalogManifestResolutionStatus::Unsupported
        },
        StoreCatalogManifestResolutionStatus::Malformed => {
            CatalogManifestResolutionStatus::Malformed
        },
        StoreCatalogManifestResolutionStatus::Internal => CatalogManifestResolutionStatus::Internal,
        StoreCatalogManifestResolutionStatus::CatalogNotReady => {
            CatalogManifestResolutionStatus::CatalogNotReady
        },
        StoreCatalogManifestResolutionStatus::AuthRequired => {
            CatalogManifestResolutionStatus::AuthRequired
        },
    }
}

/// Projects a generated Protobuf status into the protocol-free catalog-store
/// runtime status. `Unspecified` is rejected at the generated boundary.
pub fn catalog_manifest_resolution_status_from_protobuf(
    status: CatalogManifestResolutionStatus,
) -> AndromedaResult<StoreCatalogManifestResolutionStatus> {
    match status {
        CatalogManifestResolutionStatus::Resolved => {
            Ok(StoreCatalogManifestResolutionStatus::Resolved)
        },
        CatalogManifestResolutionStatus::NotFound => {
            Ok(StoreCatalogManifestResolutionStatus::NotFound)
        },
        CatalogManifestResolutionStatus::CatalogVersionMismatch => {
            Ok(StoreCatalogManifestResolutionStatus::CatalogVersionMismatch)
        },
        CatalogManifestResolutionStatus::ContractHashMismatch => {
            Ok(StoreCatalogManifestResolutionStatus::ContractHashMismatch)
        },
        CatalogManifestResolutionStatus::NotSourceGeneratorReady => {
            Ok(StoreCatalogManifestResolutionStatus::NotSourceGeneratorReady)
        },
        CatalogManifestResolutionStatus::PermissionDenied => {
            Ok(StoreCatalogManifestResolutionStatus::PermissionDenied)
        },
        CatalogManifestResolutionStatus::Unsupported => {
            Ok(StoreCatalogManifestResolutionStatus::Unsupported)
        },
        CatalogManifestResolutionStatus::Malformed => {
            Ok(StoreCatalogManifestResolutionStatus::Malformed)
        },
        CatalogManifestResolutionStatus::Internal => {
            Ok(StoreCatalogManifestResolutionStatus::Internal)
        },
        CatalogManifestResolutionStatus::CatalogNotReady => {
            Ok(StoreCatalogManifestResolutionStatus::CatalogNotReady)
        },
        CatalogManifestResolutionStatus::AuthRequired => {
            Ok(StoreCatalogManifestResolutionStatus::AuthRequired)
        },
        CatalogManifestResolutionStatus::Unspecified => Err(protocol_error(
            "catalog manifest resolution status must be specified",
        )),
    }
}

/// Projects a generated Protobuf status code into the protocol-free
/// catalog-store runtime status.
pub fn catalog_manifest_resolution_status_from_protobuf_i32(
    status: i32,
) -> AndromedaResult<StoreCatalogManifestResolutionStatus> {
    match status {
        value if value == CatalogManifestResolutionStatus::Resolved as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::Resolved)
        },
        value if value == CatalogManifestResolutionStatus::NotFound as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::NotFound)
        },
        value if value == CatalogManifestResolutionStatus::CatalogVersionMismatch as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::CatalogVersionMismatch)
        },
        value if value == CatalogManifestResolutionStatus::ContractHashMismatch as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::ContractHashMismatch)
        },
        value if value == CatalogManifestResolutionStatus::NotSourceGeneratorReady as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::NotSourceGeneratorReady)
        },
        value if value == CatalogManifestResolutionStatus::PermissionDenied as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::PermissionDenied)
        },
        value if value == CatalogManifestResolutionStatus::Unsupported as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::Unsupported)
        },
        value if value == CatalogManifestResolutionStatus::Malformed as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::Malformed)
        },
        value if value == CatalogManifestResolutionStatus::Internal as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::Internal)
        },
        value if value == CatalogManifestResolutionStatus::CatalogNotReady as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::CatalogNotReady)
        },
        value if value == CatalogManifestResolutionStatus::AuthRequired as i32 => {
            Ok(StoreCatalogManifestResolutionStatus::AuthRequired)
        },
        value if value == CatalogManifestResolutionStatus::Unspecified as i32 => Err(
            protocol_error("catalog manifest resolution status must be specified"),
        ),
        _ => Err(protocol_error("unknown catalog manifest resolution status")),
    }
}

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

/// Decoded catalog manifest resolution request evidence from a command frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolutionFrameRequest {
    pub envelope: ProtoFrameEnvelope,
    pub request: CatalogManifestResolutionRequest,
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
    validate_catalog_frame(frame, FrameType::ContractRequest)?;

    let envelope = decode_catalog_envelope(frame, PayloadKind::ContractRequest)?;
    let request: CatalogProcedureManifestResolutionRequest =
        decode_generated_message(envelope.payload.as_slice())?;
    validate_catalog_procedure_manifest_resolution_request(&request)?;
    validate_catalog_manifest_resolution_request_context(frame, &envelope, request.request_id)?;

    Ok(request)
}

/// Decodes and projects a catalog manifest resolution request for a route runtime.
pub fn decode_catalog_manifest_resolution_route_request(
    frame: &FrameBytes,
) -> AndromedaResult<CatalogManifestResolutionFrameRequest> {
    validate_catalog_frame(frame, FrameType::ContractRequest)?;

    let envelope = decode_catalog_envelope(frame, PayloadKind::ContractRequest)?;
    let generated_request: GeneratedCatalogManifestResolutionRequest =
        decode_generated_message(envelope.payload.as_slice())?;
    validate_catalog_procedure_manifest_resolution_request(&generated_request)?;
    validate_catalog_manifest_resolution_request_context(
        frame,
        &envelope,
        generated_request.request_id,
    )?;

    Ok(CatalogManifestResolutionFrameRequest {
        envelope,
        request: CatalogManifestResolutionRequest::from_protobuf(generated_request)?,
    })
}

/// Decodes and validates a catalog manifest resolution response frame.
pub fn decode_catalog_manifest_resolution_response_frame(
    frame: &FrameBytes,
) -> AndromedaResult<CatalogProcedureManifestResolutionResponse> {
    validate_catalog_frame(frame, FrameType::ContractResponse)?;

    let envelope = decode_catalog_envelope(frame, PayloadKind::ContractResponse)?;
    let response: CatalogProcedureManifestResolutionResponse =
        decode_generated_message(envelope.payload.as_slice())?;
    validate_catalog_procedure_manifest_resolution_response(&response)?;
    validate_catalog_manifest_resolution_request_context(frame, &envelope, response.request_id)?;

    Ok(response)
}

/// Encodes a route runtime response into a `ContractResponse` frame.
pub fn encode_catalog_manifest_resolution_response_frame(
    request_envelope: &ProtoFrameEnvelope,
    request_header: FrameHeader,
    response: &CatalogManifestResolutionResponse,
) -> AndromedaResult<FrameBytes> {
    validate_catalog_manifest_resolution_response_context(request_header.request_id, response)?;
    let generated_response = response.to_protobuf()?;
    encode_catalog_response_frame(request_envelope, request_header, &generated_response)
}

/// Validates request id lockstep across the frame, typed envelope, and payload.
pub fn validate_catalog_manifest_resolution_request_context(
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

/// Validates that a runtime response remains bound to the admitted request id.
pub fn validate_catalog_manifest_resolution_response_context(
    expected_request_id: RequestId,
    response: &CatalogManifestResolutionResponse,
) -> AndromedaResult<()> {
    if response.request_id != expected_request_id {
        return Err(protocol_error(
            "catalog manifest resolution runtime changed response request id",
        ));
    }

    Ok(())
}

fn validate_catalog_frame(frame: &FrameBytes, expected_type: FrameType) -> AndromedaResult<()> {
    validate_single_frame_on_stream(frame, StreamRole::CommandBidirectional)?;

    if frame.header.frame_type != expected_type {
        return Err(protocol_error(match expected_type {
            FrameType::ContractRequest => {
                "catalog manifest resolution request decoder requires ContractRequest frame"
            },
            FrameType::ContractResponse => {
                "catalog manifest resolution response decoder requires ContractResponse frame"
            },
            _ => "catalog manifest resolution decoder received unexpected frame type",
        }));
    }

    Ok(())
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
    let envelope = project_generated_frame_envelope(&generated_envelope)?;

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

/// Runtime-free domain selector for catalog manifest resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogManifestSelector {
    ProcedureId(ProcedureId),
    ProcedureName(String),
}

impl CatalogManifestSelector {
    fn validate_resolved_manifest(
        &self,
        manifest: &CatalogProcedureManifest,
    ) -> AndromedaResult<()> {
        match self {
            CatalogManifestSelector::ProcedureId(expected) => {
                if manifest.procedure_id != *expected {
                    return Err(contract_error(
                        "resolved catalog manifest procedure id does not match request selector",
                    ));
                }
            },
            CatalogManifestSelector::ProcedureName(expected) => {
                if manifest.procedure_name != *expected {
                    return Err(contract_error(
                        "resolved catalog manifest procedure name does not match request selector",
                    ));
                }
            },
        }

        Ok(())
    }
}

/// Domain request used after protobuf boundary decoding.
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
            },
            Some(GeneratedCatalogManifestSelector::ProcedureName(procedure_name)) => {
                CatalogManifestSelector::ProcedureName(procedure_name)
            },
            None => {
                return Err(protocol_error(
                    "catalog manifest resolution request requires selector",
                ));
            },
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

/// Domain response returned by a catalog manifest runtime before protobuf encoding.
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
            .map(catalog_procedure_manifest_to_protobuf)
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

        request.selector.validate_resolved_manifest(manifest)?;

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

fn catalog_procedure_manifest_to_protobuf(
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<GeneratedProcedureManifest> {
    let manifest = GeneratedProcedureManifest {
        procedure_id: manifest.procedure_id.get(),
        procedure_name: manifest.procedure_name.clone(),
        contract_hash: manifest.contract_hash.as_bytes().to_vec(),
        catalog_version: manifest.catalog_version.get(),
        protocol_layout: Some(catalog_protocol_layout_to_protobuf(
            &manifest.protocol_layout,
        )),
        result_streams: manifest
            .result_streams
            .iter()
            .map(catalog_result_stream_to_protobuf)
            .collect(),
        policy_version: manifest.policy_version.as_bytes().to_vec(),
        required_permissions: manifest
            .required_permissions
            .iter()
            .map(catalog_required_permission_to_protobuf)
            .collect(),
        stats_version: Some(manifest.stats_version),
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

pub(crate) fn validate_catalog_procedure_manifest_projection(
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<()> {
    catalog_procedure_manifest_to_protobuf(manifest).map(|_| ())
}

fn catalog_protocol_layout_to_protobuf(
    layout: &CatalogProcedureProtocolLayout,
) -> GeneratedProtocolLayout {
    GeneratedProtocolLayout {
        descriptor_set_hash: layout.descriptor_set_hash.as_bytes().to_vec(),
        frame_envelope_hash: layout.frame_envelope_hash.as_bytes().to_vec(),
        protocol_package: layout.protocol_package.clone(),
        contract_package: layout.contract_package.clone(),
    }
}

fn catalog_required_permission_to_protobuf(
    permission: &CatalogRequiredPermission,
) -> GeneratedRequiredPermission {
    GeneratedRequiredPermission {
        id: permission.id.clone(),
        family: permission.family.clone(),
    }
}

fn catalog_result_stream_to_protobuf(
    stream: &CatalogResultStreamDescriptor,
) -> GeneratedResultStreamDescriptor {
    GeneratedResultStreamDescriptor {
        stream_name: stream.stream_name.clone(),
        columns: stream
            .columns
            .iter()
            .map(catalog_column_to_protobuf)
            .collect(),
        cardinality: stream.cardinality,
        row_count_requirement: stream.row_count_requirement,
        row_count_exact: stream.row_count_exact,
        row_count_max: stream.row_count_max,
    }
}

fn catalog_column_to_protobuf(column: &CatalogColumnDescriptor) -> GeneratedColumnDescriptor {
    GeneratedColumnDescriptor {
        name: column.name.clone(),
        ordinal: column.ordinal,
        type_name: column.type_name.clone(),
    }
}

fn protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

fn contract_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_error::AndromedaErrorKind;

    #[test]
    fn store_status_projects_to_generated_protobuf_status() {
        assert_eq!(
            catalog_manifest_resolution_status_to_protobuf(
                StoreCatalogManifestResolutionStatus::NotSourceGeneratorReady
            ),
            CatalogManifestResolutionStatus::NotSourceGeneratorReady
        );
        assert_eq!(
            catalog_manifest_resolution_status_to_protobuf(
                StoreCatalogManifestResolutionStatus::AuthRequired
            ),
            CatalogManifestResolutionStatus::AuthRequired
        );
    }

    #[test]
    fn generated_status_projects_to_store_status() {
        assert_eq!(
            catalog_manifest_resolution_status_from_protobuf(
                CatalogManifestResolutionStatus::PermissionDenied
            )
            .unwrap(),
            StoreCatalogManifestResolutionStatus::PermissionDenied
        );
        assert_eq!(
            catalog_manifest_resolution_status_from_protobuf_i32(
                CatalogManifestResolutionStatus::CatalogNotReady as i32
            )
            .unwrap(),
            StoreCatalogManifestResolutionStatus::CatalogNotReady
        );
    }

    #[test]
    fn generated_status_projection_rejects_unspecified_and_unknown_codes() {
        let unspecified = catalog_manifest_resolution_status_from_protobuf(
            CatalogManifestResolutionStatus::Unspecified,
        )
        .unwrap_err();
        assert_eq!(unspecified.kind(), AndromedaErrorKind::Protocol);
        assert!(unspecified.message().contains("must be specified"));

        let unspecified_i32 = catalog_manifest_resolution_status_from_protobuf_i32(0).unwrap_err();
        assert_eq!(unspecified_i32.kind(), AndromedaErrorKind::Protocol);

        let unknown_i32 = catalog_manifest_resolution_status_from_protobuf_i32(99).unwrap_err();
        assert_eq!(unknown_i32.kind(), AndromedaErrorKind::Protocol);
        assert!(unknown_i32.message().contains("unknown"));
    }
}
