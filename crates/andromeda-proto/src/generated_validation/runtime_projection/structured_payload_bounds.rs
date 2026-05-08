use andromeda_error::AndromedaResult;
use andromeda_proto_wire::{
    validate_result_batch_payload_parts,
    validate_rpc_execute_argument_value as validate_codec_argument_value,
    validate_rpc_execute_arguments_len as validate_codec_arguments_len,
    validate_rpc_response_sequence_len as validate_codec_response_sequence_len,
};

use crate::generated::protocol;

pub(super) use andromeda_proto_wire::StructuredPayloadByteTracker;

pub(super) fn validate_rpc_execute_arguments_len(arguments_len: usize) -> AndromedaResult<()> {
    validate_codec_arguments_len(arguments_len)
}

pub(super) fn validate_rpc_execute_argument_value(value: &[u8]) -> AndromedaResult<()> {
    validate_codec_argument_value(value)
}

pub(super) fn validate_rpc_response_sequence_len(responses_len: usize) -> AndromedaResult<()> {
    validate_codec_response_sequence_len(responses_len)
}

pub(super) fn validate_rpc_batch_payload(batch: &protocol::v1::RpcBatch) -> AndromedaResult<()> {
    validate_result_batch_payload_parts(
        &batch.structured_payload,
        batch.rows_emitted,
        batch.row_count_exact,
    )
}
