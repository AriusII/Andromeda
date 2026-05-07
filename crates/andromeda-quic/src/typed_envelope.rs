//! Typed Protobuf envelope validation for QUIC frames.
//!
//! QUIC frame headers and Protobuf payload envelopes are both part of the
//! native Andromeda wire contract. This module validates their lockstep before
//! a frame can be treated as typed RPC evidence.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, RequestId,
    SessionId, TransactionId,
};
use andromeda_proto::{
    FrameEnvelope as ProtoFrameEnvelope, PayloadKind, ProtocolVersion,
    RpcResultStreamMetadataPolicy, decode_generated_message, generated,
};

use crate::{FrameBytes, ResultStreamMetadataPolicy, ResultStreamSequence};

type GeneratedFrameEnvelope = generated::protocol::v1::FrameEnvelope;

/// Default V0 upper bound for one typed ResultStream.
///
/// The wire already bounds each frame payload. This stream-level cap prevents a
/// caller from validating an unbounded sequence of otherwise valid frames in a
/// single ResultStream contract check.
pub const DEFAULT_MAX_TYPED_RESULT_STREAM_FRAMES: usize = 1_024;

/// Default V0 upper bound for encoded typed envelope bytes in one ResultStream.
pub const DEFAULT_MAX_TYPED_RESULT_STREAM_ENVELOPE_BYTES: u64 = 64 * 1024 * 1024;

/// Expected route context for a typed RPC ResultStream.
///
/// ResultStream frames are only valid when they stay bound to the Procedure
/// invocation context admitted before executor dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypedResultStreamContext {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub tx_id: Option<TransactionId>,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
}

impl TypedResultStreamContext {
    pub const fn new(
        request_id: RequestId,
        session_id: SessionId,
        tx_id: Option<TransactionId>,
        contract_hash: ContractHash,
        catalog_version: CatalogVersion,
    ) -> Self {
        Self {
            request_id,
            session_id,
            tx_id,
            contract_hash,
            catalog_version,
        }
    }

    pub const fn from_envelope(envelope: &ProtoFrameEnvelope) -> Self {
        Self {
            request_id: envelope.request_id,
            session_id: envelope.session_id,
            tx_id: envelope.tx_id,
            contract_hash: envelope.contract_hash,
            catalog_version: envelope.catalog_version,
        }
    }

    pub fn validate_frame_envelope(self, frame: &FrameBytes) -> AndromedaResult<()> {
        let envelope = decode_typed_frame_envelope(frame)?;
        if Self::from_envelope(&envelope) != self {
            return Err(protocol_error(
                "typed result-stream envelope does not match expected route context",
            ));
        }

        Ok(())
    }
}

/// Bounded validation policy for a typed RPC ResultStream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypedResultStreamBounds {
    pub max_frame_count: usize,
    pub max_envelope_payload_bytes: u64,
}

impl TypedResultStreamBounds {
    pub const fn v0_default() -> Self {
        Self {
            max_frame_count: DEFAULT_MAX_TYPED_RESULT_STREAM_FRAMES,
            max_envelope_payload_bytes: DEFAULT_MAX_TYPED_RESULT_STREAM_ENVELOPE_BYTES,
        }
    }

    pub const fn new(max_frame_count: usize, max_envelope_payload_bytes: u64) -> Self {
        Self {
            max_frame_count,
            max_envelope_payload_bytes,
        }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.max_frame_count == 0 {
            return Err(resource_error(
                "typed result-stream bounds require a nonzero frame count",
            ));
        }

        if self.max_envelope_payload_bytes == 0 {
            return Err(resource_error(
                "typed result-stream bounds require a nonzero byte budget",
            ));
        }

        Ok(())
    }
}

impl Default for TypedResultStreamBounds {
    fn default() -> Self {
        Self::v0_default()
    }
}

/// Decode and validate a Protobuf `FrameEnvelope` carried by a QUIC frame.
///
/// The function rejects protocol drift before dispatch:
/// - missing or unsupported `ProtocolVersion`;
/// - payload kind not matching the QUIC `FrameType`;
/// - request/session/transaction context drift between header and envelope;
/// - envelope-level contract and payload invariants.
pub fn decode_typed_frame_envelope(frame: &FrameBytes) -> AndromedaResult<ProtoFrameEnvelope> {
    if frame.header.frame_type.allows_datagram() {
        return Err(protocol_error(
            "telemetry datagram frames do not carry typed RPC envelopes",
        ));
    }

    let generated_envelope: GeneratedFrameEnvelope = decode_generated_message(&frame.payload)?;
    let Some(protocol_version) = generated_envelope.protocol_version else {
        return Err(protocol_error(
            "typed frame envelope requires protocol version",
        ));
    };
    let payload_kind = PayloadKind::try_from(generated_envelope.payload_kind as u32)?;

    let envelope = ProtoFrameEnvelope {
        protocol_version: ProtocolVersion {
            major: protocol_version.major,
            minor: protocol_version.minor,
        },
        contract_hash: ContractHash::from_slice(&generated_envelope.contract_hash)?,
        catalog_version: CatalogVersion::new(generated_envelope.catalog_version),
        request_id: RequestId::new(generated_envelope.request_id),
        session_id: SessionId::new(generated_envelope.session_id),
        tx_id: generated_envelope.tx_id.map(TransactionId::new),
        payload_kind,
        payload: generated_envelope.payload,
    };
    envelope.validate()?;

    if envelope.payload_kind.wire_code() != frame.header.frame_type.wire_code() {
        return Err(protocol_error(
            "typed frame envelope payload kind does not match frame type",
        ));
    }

    if envelope.request_id != frame.header.request_id
        || envelope.session_id != frame.header.session_id
        || envelope.tx_id != frame.header.tx_id
    {
        return Err(protocol_error(
            "typed frame envelope context does not match frame header",
        ));
    }

    Ok(envelope)
}

/// Validate QUIC result-stream frame order and the typed Protobuf envelope
/// order in the same pass.
pub fn validate_typed_result_stream_sequence(frames: &[FrameBytes]) -> AndromedaResult<()> {
    validate_typed_result_stream_sequence_with_metadata_policy(
        frames,
        ResultStreamMetadataPolicy::RowBatchRequired,
    )
}

/// Validate a typed result-stream sequence with an explicit completion policy.
pub fn validate_typed_result_stream_sequence_with_metadata_policy(
    frames: &[FrameBytes],
    metadata_policy: ResultStreamMetadataPolicy,
) -> AndromedaResult<()> {
    validate_typed_result_stream_sequence_internal(
        frames,
        metadata_policy,
        None,
        TypedResultStreamBounds::v0_default(),
    )
}

/// Validate a typed ResultStream sequence against the admitted invocation
/// context and an explicit bounded stream policy.
pub fn validate_typed_result_stream_sequence_with_context_and_bounds(
    frames: &[FrameBytes],
    metadata_policy: ResultStreamMetadataPolicy,
    expected_context: TypedResultStreamContext,
    bounds: TypedResultStreamBounds,
) -> AndromedaResult<()> {
    validate_typed_result_stream_sequence_internal(
        frames,
        metadata_policy,
        Some(expected_context),
        bounds,
    )
}

fn validate_typed_result_stream_sequence_internal(
    frames: &[FrameBytes],
    metadata_policy: ResultStreamMetadataPolicy,
    expected_context: Option<TypedResultStreamContext>,
    bounds: TypedResultStreamBounds,
) -> AndromedaResult<()> {
    bounds.validate()?;
    if frames.len() > bounds.max_frame_count {
        return Err(resource_error(
            "typed result-stream frame count exceeds bounded policy",
        ));
    }

    let mut frame_sequence = ResultStreamSequence::new_with_metadata_policy(metadata_policy);
    let mut envelopes = Vec::with_capacity(frames.len());
    let mut total_envelope_payload_bytes = 0_u64;

    for frame in frames {
        total_envelope_payload_bytes = total_envelope_payload_bytes
            .checked_add(frame.payload.len() as u64)
            .ok_or_else(|| resource_error("typed result-stream byte budget overflow"))?;
        if total_envelope_payload_bytes > bounds.max_envelope_payload_bytes {
            return Err(resource_error(
                "typed result-stream envelope bytes exceed bounded policy",
            ));
        }

        frame_sequence.accept(frame)?;
        let envelope = decode_typed_frame_envelope(frame)?;
        if let Some(expected_context) = expected_context
            && TypedResultStreamContext::from_envelope(&envelope) != expected_context
        {
            return Err(protocol_error(
                "typed result-stream envelope does not match expected route context",
            ));
        }
        envelopes.push(envelope);
    }

    if !frame_sequence.is_complete() {
        return Err(protocol_error("typed result-stream sequence is incomplete"));
    }

    ProtoFrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
        &envelopes,
        proto_metadata_policy(metadata_policy),
    )
}

const fn proto_metadata_policy(
    metadata_policy: ResultStreamMetadataPolicy,
) -> RpcResultStreamMetadataPolicy {
    match metadata_policy {
        ResultStreamMetadataPolicy::RowBatchRequired => {
            RpcResultStreamMetadataPolicy::RowBatchRequired
        }
        ResultStreamMetadataPolicy::ZeroRowCompletionAllowed => {
            RpcResultStreamMetadataPolicy::ZeroRowCompletionAllowed
        }
        ResultStreamMetadataPolicy::MutationOnly => RpcResultStreamMetadataPolicy::MutationOnly,
    }
}

fn protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

fn resource_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Resource, message)
}
