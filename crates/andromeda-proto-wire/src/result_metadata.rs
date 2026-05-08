use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

const MAX_GENERATED_RPC_EXECUTE_ARGUMENTS: usize = 128;
const MAX_GENERATED_RPC_ARGUMENT_VALUE_BYTES: usize = 16 * 1024 * 1024;
const MAX_GENERATED_INVOCATION_RESPONSE_FRAMES: usize = 1_024;
const MAX_GENERATED_INVOCATION_RESPONSE_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;

pub fn validate_rpc_execute_arguments_len(arguments_len: usize) -> AndromedaResult<()> {
    if arguments_len > MAX_GENERATED_RPC_EXECUTE_ARGUMENTS {
        return contract_error(format!(
            "generated RPC execute request arguments exceed bounded limit of {MAX_GENERATED_RPC_EXECUTE_ARGUMENTS}"
        ));
    }

    Ok(())
}

pub fn validate_rpc_execute_argument_value(value: &[u8]) -> AndromedaResult<()> {
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

pub fn validate_rpc_response_sequence_len(responses_len: usize) -> AndromedaResult<()> {
    if responses_len > MAX_GENERATED_INVOCATION_RESPONSE_FRAMES {
        return protocol_error(format!(
            "generated invocation response sequence frame count exceeds bounded limit of {MAX_GENERATED_INVOCATION_RESPONSE_FRAMES}"
        ));
    }

    Ok(())
}

pub fn validate_result_batch_payload_parts(
    structured_payload: &[u8],
    rows_emitted: u64,
    row_count_exact: Option<u64>,
) -> AndromedaResult<()> {
    if structured_payload.is_empty() {
        return protocol_error("generated RPC batch structured_payload must be non-empty");
    }

    if rows_emitted == 0 {
        return contract_error("generated RPC batch rows_emitted must be nonzero");
    }

    if let Some(exact) = row_count_exact
        && exact < rows_emitted
    {
        return contract_error("generated RPC batch row_count_exact is below rows_emitted");
    }

    Ok(())
}

#[derive(Default)]
pub struct StructuredPayloadByteTracker {
    total_structured_payload_bytes: u64,
}

impl StructuredPayloadByteTracker {
    pub fn observe_batch_payload(&mut self, payload: &[u8]) -> AndromedaResult<()> {
        let payload_len = u64::try_from(payload.len()).map_err(|_| {
            AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "generated invocation response sequence structured_payload length does not fit u64",
            )
        })?;
        self.total_structured_payload_bytes = self
            .total_structured_payload_bytes
            .checked_add(payload_len)
            .ok_or_else(|| {
                AndromedaError::new(
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

fn contract_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Contract,
        message.into(),
    ))
}

fn protocol_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Protocol,
        message.into(),
    ))
}
