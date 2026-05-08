use andromeda_error::AndromedaResult;

use crate::FrameEnvelope;
use crate::generated::protocol;

mod invocation_response_sequence;

pub use self::invocation_response_sequence::validate_generated_invocation_response_sequence;

pub fn project_generated_frame_envelope(
    envelope: &protocol::v1::FrameEnvelope,
) -> AndromedaResult<FrameEnvelope> {
    andromeda_proto_wire::generated_validation::project_generated_frame_envelope(envelope)
}

pub fn validate_generated_frame_envelope(
    envelope: &protocol::v1::FrameEnvelope,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_frame_envelope(envelope)
}

pub fn validate_generated_rpc_metadata(
    metadata: &protocol::v1::RpcMetadata,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_rpc_metadata(metadata)
}

pub fn validate_generated_rpc_batch(batch: &protocol::v1::RpcBatch) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_rpc_batch(batch)
}

pub fn validate_generated_invocation_response(
    response: &protocol::v1::InvocationResponse,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_invocation_response(response)
}
