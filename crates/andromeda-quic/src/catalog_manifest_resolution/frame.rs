use andromeda_core::{
    AndromedaResult, CatalogVersion, ContractHash, RequestId, SessionId, TransactionId,
};
use andromeda_proto::{
    FrameEnvelope as ProtoFrameEnvelope, PayloadKind, ProtocolVersion, decode_generated_message,
    encode_generated_message, validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response,
};

use crate::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader, FrameType, StreamRole, TransportSurface,
    validate_transport_surface,
};

use super::errors::protocol_error;
use super::validation::validate_request_context;
use super::{
    CatalogProcedureManifestResolutionRequest, CatalogProcedureManifestResolutionResponse,
    GeneratedFrameEnvelope, GeneratedProtocolVersion,
};

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

pub(super) fn encode_catalog_response_frame(
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

pub(super) fn decode_catalog_envelope(
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
