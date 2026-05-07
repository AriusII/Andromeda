use andromeda_error::{AndromedaErrorKind, AndromedaResult};

use crate::generated::protocol;

use super::super::{contract_error, protocol_error};

const MAX_GENERATED_RPC_EXECUTE_ARGUMENTS: usize = 128;
const MAX_GENERATED_RPC_ARGUMENT_VALUE_BYTES: usize = 16 * 1024 * 1024;
const MAX_GENERATED_INVOCATION_RESPONSE_FRAMES: usize = 1_024;
const MAX_GENERATED_INVOCATION_RESPONSE_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;

pub(super) fn validate_rpc_execute_arguments_len(arguments_len: usize) -> AndromedaResult<()> {
    if arguments_len > MAX_GENERATED_RPC_EXECUTE_ARGUMENTS {
        return contract_error(format!(
            "generated RPC execute request arguments exceed bounded limit of {MAX_GENERATED_RPC_EXECUTE_ARGUMENTS}"
        ));
    }

    Ok(())
}

pub(super) fn validate_rpc_execute_argument_value(value: &[u8]) -> AndromedaResult<()> {
    if value.is_empty() {
        return contract_error("generated RPC execute argument value must be non-empty binary");
    }
    if value.len() > MAX_GENERATED_RPC_ARGUMENT_VALUE_BYTES {
        return contract_error(format!(
            "generated RPC execute argument value exceeds bounded limit of {MAX_GENERATED_RPC_ARGUMENT_VALUE_BYTES} bytes"
        ));
    }

    Ok(())
}

pub(super) fn validate_rpc_response_sequence_len(responses_len: usize) -> AndromedaResult<()> {
    if responses_len > MAX_GENERATED_INVOCATION_RESPONSE_FRAMES {
        return protocol_error(format!(
            "generated invocation response sequence frame count exceeds bounded limit of {MAX_GENERATED_INVOCATION_RESPONSE_FRAMES}"
        ));
    }

    Ok(())
}

pub(super) fn validate_rpc_batch_payload(batch: &protocol::v1::RpcBatch) -> AndromedaResult<()> {
    if batch.structured_payload.is_empty() {
        return protocol_error("generated RPC batch structured_payload must be non-empty");
    }

    if batch.rows_emitted == 0 {
        return contract_error("generated RPC batch rows_emitted must be nonzero");
    }

    if let Some(exact) = batch.row_count_exact
        && exact < batch.rows_emitted
    {
        return contract_error("generated RPC batch row_count_exact is below rows_emitted");
    }

    Ok(())
}

#[derive(Default)]
pub(super) struct StructuredPayloadByteTracker {
    total_structured_payload_bytes: u64,
}

impl StructuredPayloadByteTracker {
    pub(super) fn observe_batch_payload(&mut self, payload: &[u8]) -> AndromedaResult<()> {
        let payload_len = u64::try_from(payload.len()).map_err(|_| {
            andromeda_error::AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "generated invocation response sequence structured_payload length does not fit u64",
            )
        })?;
        self.total_structured_payload_bytes = self
            .total_structured_payload_bytes
            .checked_add(payload_len)
            .ok_or_else(|| {
                andromeda_error::AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "generated invocation response sequence structured_payload byte count overflow",
                )
            })?;
        if self.total_structured_payload_bytes > MAX_GENERATED_INVOCATION_RESPONSE_PAYLOAD_BYTES {
            return protocol_error(format!(
                "generated invocation response sequence structured_payload bytes exceed bounded limit of {MAX_GENERATED_INVOCATION_RESPONSE_PAYLOAD_BYTES}"
            ));
        }

        Ok(())
    }
}
