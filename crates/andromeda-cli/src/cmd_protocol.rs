//! Protocol smoke test command.

use crate::error::protocol_error;
use andromeda_admission::{InvocationContext, InvocationRequest};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_exec::LocalVerticalRuntime;
use andromeda_inventory_demo::inventory_reserve_stock_contract;
use andromeda_inventory_demo::{
    InventoryReserveStockExecutor, InventoryStock, ReserveStockCommand,
};
use andromeda_observe::TraceId;
use andromeda_result_stream::CompletionStatus;
use andromeda_types::InvocationId;
use andromeda_wal::InMemoryWal;

const PROTO_PAYLOAD_SOURCE: &str = include_str!("../../andromeda-proto-wire/src/envelope.rs");
const QUIC_FRAME_SOURCE: &str = include_str!("../../andromeda-rpc-protocol/src/frame_code.rs");

const PROTOCOL_CODE_LOCKSTEP: &[ProtocolCode] = &[
    ProtocolCode {
        name: "Hello",
        payload_const: "HELLO_WIRE_CODE",
        frame_const: "HELLO_FRAME_CODE",
        code: 1,
    },
    ProtocolCode {
        name: "Auth",
        payload_const: "AUTH_WIRE_CODE",
        frame_const: "AUTH_FRAME_CODE",
        code: 2,
    },
    ProtocolCode {
        name: "ContractRequest",
        payload_const: "CONTRACT_REQUEST_WIRE_CODE",
        frame_const: "CONTRACT_REQUEST_FRAME_CODE",
        code: 3,
    },
    ProtocolCode {
        name: "ContractResponse",
        payload_const: "CONTRACT_RESPONSE_WIRE_CODE",
        frame_const: "CONTRACT_RESPONSE_FRAME_CODE",
        code: 4,
    },
    ProtocolCode {
        name: "RpcExecuteRequest",
        payload_const: "RPC_EXECUTE_REQUEST_WIRE_CODE",
        frame_const: "RPC_EXECUTE_REQUEST_FRAME_CODE",
        code: 5,
    },
    ProtocolCode {
        name: "RpcMetadata",
        payload_const: "RPC_METADATA_WIRE_CODE",
        frame_const: "RPC_METADATA_FRAME_CODE",
        code: 6,
    },
    ProtocolCode {
        name: "RpcBatch",
        payload_const: "RPC_BATCH_WIRE_CODE",
        frame_const: "RPC_BATCH_FRAME_CODE",
        code: 7,
    },
    ProtocolCode {
        name: "RpcCompletion",
        payload_const: "RPC_COMPLETION_WIRE_CODE",
        frame_const: "RPC_COMPLETION_FRAME_CODE",
        code: 8,
    },
    ProtocolCode {
        name: "Error",
        payload_const: "ERROR_WIRE_CODE",
        frame_const: "ERROR_FRAME_CODE",
        code: 9,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProtocolCode {
    name: &'static str,
    payload_const: &'static str,
    frame_const: &'static str,
    code: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmokeFrameKind {
    Metadata,
    Batch,
    Completion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SmokeFrame {
    kind: SmokeFrameKind,
    request_id: u64,
    session_id: u64,
    tx_id: Option<u64>,
    payload_len: usize,
}

/// Runs protocol smoke test.
pub fn run_protocol_smoke(detailed: bool) -> AndromedaResult<String> {
    let lockstep_count = validate_payload_frame_code_lockstep()?;
    validate_telemetry_datagram_policy()?;

    let result_frames = representative_result_frames();
    validate_result_frame_sequence(&result_frames)?;

    let contract_summary = validate_completion_error_structured_contract()?;

    let mut report = String::new();
    report.push_str("Andromeda protocol smoke\n");
    report.push_str(&format!(
        "payload/frame lockstep: ok ({lockstep_count} codes)\n"
    ));
    report.push_str("typed ResultStream sequence: ok (metadata -> batch -> completion)\n");
    report.push_str("telemetry datagram policy: ok (TelemetrySoftSignal=100 datagram-only)\n");
    report.push_str(&format!(
        "completion/error/structured contract: ok ({contract_summary})\n"
    ));
    if detailed {
        report.push_str("payload/frame codes:\n");
        for expected in PROTOCOL_CODE_LOCKSTEP {
            report.push_str(&format!(
                "  - {}: payload={}, frame={}, code={}\n",
                expected.name, expected.payload_const, expected.frame_const, expected.code
            ));
        }
        report.push_str("representative ResultStream frames:\n");
        for frame in result_frames {
            report.push_str(&format!(
                "  - {:?}: request={}, session={}, tx={:?}, payload_len={}\n",
                frame.kind, frame.request_id, frame.session_id, frame.tx_id, frame.payload_len
            ));
        }
    }
    report.push_str("network sockets: not opened\n");

    Ok(report)
}

fn validate_payload_frame_code_lockstep() -> AndromedaResult<usize> {
    for expected in PROTOCOL_CODE_LOCKSTEP {
        let payload_code = source_const_u32(PROTO_PAYLOAD_SOURCE, expected.payload_const)?;
        let frame_code = source_const_u32(QUIC_FRAME_SOURCE, expected.frame_const)?;

        if payload_code != expected.code
            || frame_code != expected.code
            || payload_code != frame_code
        {
            return Err(protocol_error(format!(
                "{} payload/frame code drift: payload={}, frame={}, expected={}",
                expected.name, payload_code, frame_code, expected.code
            )));
        }
    }

    Ok(PROTOCOL_CODE_LOCKSTEP.len())
}

fn validate_telemetry_datagram_policy() -> AndromedaResult<()> {
    let telemetry_code = source_const_u32(QUIC_FRAME_SOURCE, "TELEMETRY_SOFT_SIGNAL_FRAME_CODE")?;
    if telemetry_code != 100 {
        return Err(protocol_error(format!(
            "TelemetrySoftSignal frame code drift: {telemetry_code}"
        )));
    }

    for expected in PROTOCOL_CODE_LOCKSTEP {
        if expected.code == telemetry_code {
            return Err(protocol_error(format!(
                "reliable protocol frame {} overlaps telemetry datagram code",
                expected.name
            )));
        }
    }

    let declares_datagram_only = QUIC_FRAME_SOURCE.contains("Self::TelemetrySoftSignal")
        && QUIC_FRAME_SOURCE.contains("StreamRole::TelemetryDatagram")
        && QUIC_FRAME_SOURCE.contains("matches!(self, Self::TelemetrySoftSignal)");

    if !declares_datagram_only {
        return Err(protocol_error(
            "TelemetrySoftSignal must be the only datagram-permitted frame",
        ));
    }

    Ok(())
}

fn representative_result_frames() -> [SmokeFrame; 3] {
    let request_id = 1;
    let session_id = 2;
    let tx_id = Some(3);

    [
        SmokeFrame {
            kind: SmokeFrameKind::Metadata,
            request_id,
            session_id,
            tx_id,
            payload_len: 0,
        },
        SmokeFrame {
            kind: SmokeFrameKind::Batch,
            request_id,
            session_id,
            tx_id,
            payload_len: 8,
        },
        SmokeFrame {
            kind: SmokeFrameKind::Completion,
            request_id,
            session_id,
            tx_id,
            payload_len: 0,
        },
    ]
}

fn validate_result_frame_sequence(frames: &[SmokeFrame]) -> AndromedaResult<()> {
    let mut context = None;
    let mut saw_metadata = false;
    let mut saw_batch = false;
    let mut completed = false;

    for frame in frames {
        let current_context = (frame.request_id, frame.session_id, frame.tx_id);
        match context {
            Some(expected_context) if expected_context != current_context => {
                return Err(protocol_error(
                    "ResultStream sequence changed request context",
                ));
            },
            None => context = Some(current_context),
            _ => {},
        }

        match frame.kind {
            SmokeFrameKind::Metadata => {
                if saw_metadata || saw_batch || completed {
                    return Err(protocol_error(
                        "ResultStream metadata must be the first frame",
                    ));
                }
                saw_metadata = true;
            },
            SmokeFrameKind::Batch => {
                if !saw_metadata {
                    return Err(protocol_error(
                        "ResultStream metadata must precede batch frames",
                    ));
                }
                if completed {
                    return Err(protocol_error(
                        "ResultStream batch must not follow completion",
                    ));
                }
                if frame.payload_len == 0 {
                    return Err(protocol_error(
                        "ResultStream batch requires a non-empty payload",
                    ));
                }
                saw_batch = true;
            },
            SmokeFrameKind::Completion => {
                if !saw_metadata || !saw_batch {
                    return Err(protocol_error(
                        "ResultStream completion requires prior metadata and batch frames",
                    ));
                }
                if completed {
                    return Err(protocol_error("ResultStream completion must appear once"));
                }
                completed = true;
            },
        }
    }

    if saw_metadata && saw_batch && completed {
        Ok(())
    } else {
        Err(protocol_error("ResultStream sequence is incomplete"))
    }
}

fn validate_completion_error_structured_contract() -> AndromedaResult<String> {
    let contract = inventory_reserve_stock_contract()?;
    let request = InvocationRequest {
        invocation_id: InvocationId::new(19),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    let structured_parameter_count = request.structured_parameters.len();

    let effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 1,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 1,
        },
    )?;
    let procedure = effect.to_local_procedure(&contract)?;

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let context = InvocationContext::new(TraceId::new(19), contract.required_permissions.clone());
    let outcome = runtime.execute_authorized(request, &procedure, &context)?;

    if outcome.completion.status() != CompletionStatus::Committed {
        return Err(protocol_error(format!(
            "unexpected completion status: {:?}",
            outcome.completion.status()
        )));
    }

    let error = AndromedaError::new(AndromedaErrorKind::Protocol, "protocol smoke diagnostic");
    if error.kind() != AndromedaErrorKind::Protocol || error.message().is_empty() {
        return Err(protocol_error(
            "protocol error contract did not preserve kind/message",
        ));
    }

    if procedure.result_metadata.column_count == 0 {
        return Err(protocol_error("result metadata must declare columns"));
    }

    Ok(format!(
        "status={:?}, error={:?}, structured_parameters={}, result_columns={}",
        outcome.completion.status(),
        error.kind(),
        structured_parameter_count,
        procedure.result_metadata.column_count
    ))
}

fn source_const_u32(source: &str, name: &str) -> AndromedaResult<u32> {
    let prefix = format!("pub const {name}: u32 = ");

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix(&prefix) {
            return value
                .trim_end_matches(';')
                .trim()
                .parse::<u32>()
                .map_err(|_| protocol_error(format!("could not parse u32 constant {name}")));
        }
    }

    Err(protocol_error(format!("missing u32 constant {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_smoke_is_deterministic_and_concise() {
        let first = run_protocol_smoke(false).unwrap();
        let second = run_protocol_smoke(false).unwrap();

        assert_eq!(first, second);
        assert!(first.contains("payload/frame lockstep: ok (9 codes)"));
        assert!(first.contains("typed ResultStream sequence: ok"));
        assert!(first.contains("telemetry datagram policy: ok"));
        assert!(first.contains("network sockets: not opened"));
    }

    #[test]
    fn protocol_smoke_detail_lists_stable_codes() {
        let report = run_protocol_smoke(true).unwrap();

        assert!(report.contains("payload/frame codes:"));
        assert!(report.contains("RpcExecuteRequest: payload=RPC_EXECUTE_REQUEST_WIRE_CODE"));
        assert!(report.contains("representative ResultStream frames:"));
    }

    #[test]
    fn protocol_smoke_sequence_rejects_batch_before_metadata() {
        let frames = representative_result_frames();
        let invalid = [frames[1], frames[0], frames[2]];

        assert_eq!(
            validate_result_frame_sequence(&invalid).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
