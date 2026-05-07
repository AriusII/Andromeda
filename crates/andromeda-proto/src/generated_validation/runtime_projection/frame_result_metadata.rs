use andromeda_core::{
    AndromedaResult, CatalogVersion, ContractHash, RequestId, SessionId, TransactionId,
};

use crate::generated::protocol;
use crate::{FrameEnvelope, PayloadKind, ProtocolVersion};

use super::super::{
    contract_error, protocol_error, validate_generated_result_streams,
    validate_generated_rpc_completion, validate_optional_contract_hash,
    validate_required_contract_hash,
};
use super::error_helpers::validate_generated_error_envelope;
use super::structured_payload_bounds::validate_rpc_batch_payload;

mod invocation_response_sequence;

pub use self::invocation_response_sequence::validate_generated_invocation_response_sequence;
use super::invocation_correlation::{
    validate_required_invocation_correlation, validate_response_correlation_ids,
};

pub fn project_generated_frame_envelope(
    envelope: &protocol::v1::FrameEnvelope,
) -> AndromedaResult<FrameEnvelope> {
    let Some(version) = envelope.protocol_version.as_ref() else {
        return protocol_error("generated frame envelope requires protocol_version");
    };

    let payload_kind = project_payload_kind(envelope.payload_kind)?;
    let contract_hash = project_contract_hash_for_payload_kind(
        "generated frame envelope contract_hash",
        payload_kind,
        &envelope.contract_hash,
    )?;

    FrameEnvelope {
        protocol_version: ProtocolVersion {
            major: version.major,
            minor: version.minor,
        },
        contract_hash,
        catalog_version: CatalogVersion::new(envelope.catalog_version),
        request_id: RequestId::new(envelope.request_id),
        session_id: SessionId::new(envelope.session_id),
        tx_id: envelope.tx_id.map(TransactionId::new),
        payload_kind,
        payload: envelope.payload.clone(),
    }
    .validated()
}

pub fn validate_generated_frame_envelope(
    envelope: &protocol::v1::FrameEnvelope,
) -> AndromedaResult<()> {
    project_generated_frame_envelope(envelope).map(|_| ())
}

pub fn validate_generated_rpc_metadata(
    metadata: &protocol::v1::RpcMetadata,
) -> AndromedaResult<()> {
    validate_generated_result_streams(&metadata.result_streams)?;

    let Some(policy) = metadata.completion_policy.as_ref() else {
        return contract_error("generated RPC metadata requires completion_policy");
    };

    validate_result_completion_policy(policy)
}

pub fn validate_generated_rpc_batch(batch: &protocol::v1::RpcBatch) -> AndromedaResult<()> {
    if batch.result_name.trim().is_empty() {
        return contract_error("generated RPC batch result_name must be non-empty");
    }

    validate_rpc_batch_payload(batch)
}

pub fn validate_generated_invocation_response(
    response: &protocol::v1::InvocationResponse,
) -> AndromedaResult<()> {
    let Some(correlation) = response.correlation.as_ref() else {
        return contract_error("generated invocation response requires correlation");
    };
    validate_required_invocation_correlation(
        "generated invocation response correlation",
        correlation,
    )?;

    use protocol::v1::invocation_response::Response;
    match response.response.as_ref() {
        Some(Response::Metadata(metadata)) => validate_generated_rpc_metadata(metadata),
        Some(Response::Batch(batch)) => validate_generated_rpc_batch(batch),
        Some(Response::Completion(completion)) => {
            validate_generated_rpc_completion(completion)?;
            validate_response_correlation_ids(
                "generated invocation completion",
                correlation,
                completion.request_id,
                completion.session_id,
            )
        }
        Some(Response::Error(error)) => {
            validate_generated_error_envelope(error)?;
            validate_response_correlation_ids(
                "generated invocation error",
                correlation,
                error.request_id,
                error.session_id,
            )
        }
        None => contract_error("generated invocation response requires a typed response payload"),
    }
}

fn validate_result_completion_policy(
    policy: &protocol::v1::ResultCompletionPolicy,
) -> AndromedaResult<()> {
    use protocol::v1::result_completion_policy::CompletionShape;

    match policy.completion_shape {
        value if value == CompletionShape::RequiresRowBatch as i32 => Ok(()),
        value if value == CompletionShape::AllowsZeroRowCompletion as i32 => Ok(()),
        value if value == CompletionShape::MutationOnly as i32 => Ok(()),
        value if value == CompletionShape::Unspecified as i32 => {
            contract_error("generated RPC metadata completion_shape must be specified")
        }
        _ => protocol_error("unknown generated RPC metadata completion_shape"),
    }
}

fn project_payload_kind(payload_kind: i32) -> AndromedaResult<PayloadKind> {
    let Ok(code) = u32::try_from(payload_kind) else {
        return protocol_error("unknown generated frame envelope payload_kind");
    };

    PayloadKind::try_from(code)
}

fn project_contract_hash_for_payload_kind(
    label: &str,
    payload_kind: PayloadKind,
    bytes: &[u8],
) -> AndromedaResult<ContractHash> {
    if bytes.is_empty() && !payload_kind.requires_contract_hash() {
        return Ok(ContractHash::zero());
    }

    if payload_kind.requires_contract_hash() {
        validate_required_contract_hash(label, bytes)?;
    } else {
        validate_optional_contract_hash(label, Some(bytes))?;
    }

    ContractHash::from_slice(bytes)
}
