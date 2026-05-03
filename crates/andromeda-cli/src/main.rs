#![forbid(unsafe_code)]

use andromeda_catalog::inventory_reserve_stock_contract;
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, HardwareProfile, InvocationId,
};
use andromeda_exec::{
    CompletionStatus, InvocationContext, InvocationRequest, LocalProcedure, LocalVerticalRuntime,
    ResultStreamMetadata,
};
use andromeda_observe::TraceId;
use andromeda_srpl::Cardinality;
use andromeda_storage::{InMemoryWal, Lsn};

const PROTO_PAYLOAD_SOURCE: &str = include_str!("../../andromeda-proto/src/payload.rs");
const QUIC_FRAME_SOURCE: &str = include_str!("../../andromeda-quic/src/frame.rs");

const WORKSPACE_CRATES: &[&str] = &[
    "andromeda-core",
    "andromeda-proto",
    "andromeda-quic",
    "andromeda-catalog",
    "andromeda-srpl",
    "andromeda-tx",
    "andromeda-storage",
    "andromeda-exec",
    "andromeda-observe",
];

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

fn main() -> AndromedaResult<()> {
    let args = std::env::args().collect::<Vec<String>>();
    if args.get(1).is_some_and(|arg| arg == "vertical") {
        return run_vertical_demo();
    }

    if args.get(1).is_some_and(|arg| arg == "protocol-smoke") {
        print!("{}", protocol_smoke_report()?);
        return Ok(());
    }

    let profile = HardwareProfile::conservative();

    println!("Andromeda Rust workspace 0.1.0");
    println!("database engine status: foundation only");
    println!("hardware profile: {:?}", profile.architecture);
    println!("crates:");

    for crate_name in WORKSPACE_CRATES {
        println!("  - {crate_name}");
    }

    println!("run `andromeda-cli vertical` for the local Phase 1 prototype");
    println!("run `andromeda-cli protocol-smoke` for local protocol contract inspection");
    Ok(())
}

fn run_vertical_demo() -> AndromedaResult<()> {
    let contract = inventory_reserve_stock_contract()?;
    let request = InvocationRequest {
        invocation_id: InvocationId::new(1),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    let procedure = LocalProcedure {
        contract: contract.as_ref(),
        required_permissions: contract.required_permissions.clone(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            column_count: contract.result_streams[0].columns.len() as u32,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"Inventory.ReserveStock demo mutation".to_vec(),
        rows_affected: 1,
    };
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let context = InvocationContext::new(TraceId::new(1), contract.required_permissions.clone());
    let outcome = runtime.execute_authorized(request, &procedure, &context)?;
    let durable_lsn = outcome
        .completion
        .durable_lsn
        .unwrap_or_else(|| Lsn::new(0));

    println!("Andromeda Phase 1 local vertical prototype");
    println!("procedure: {}", contract.object.name.as_catalog_path());
    println!("status: {:?}", outcome.completion.status);
    println!("rows affected: {:?}", outcome.completion.rows_affected);
    println!("durable WAL LSN: {}", durable_lsn.get());
    println!(
        "durable WAL records: {}",
        runtime.wal().replay_durable().len()
    );
    println!(
        "contract trace: {:?} ({})",
        outcome.contract_trace.decision, outcome.contract_trace.reason
    );
    if let Some(trace) = &outcome.authorization_trace {
        println!(
            "authorization trace: {:?} ({})",
            trace.decision, trace.reason
        );
    }

    debug_assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    Ok(())
}

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

fn protocol_smoke_report() -> AndromedaResult<String> {
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
    report.push_str("result stream sequence: ok (metadata -> batch -> completion)\n");
    report.push_str("telemetry datagram policy: ok (TelemetrySoftSignal=100 datagram-only)\n");
    report.push_str(&format!(
        "completion/error/structured contract: ok ({contract_summary})\n"
    ));
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
                    "result-stream sequence changed request context",
                ));
            }
            None => context = Some(current_context),
            _ => {}
        }

        match frame.kind {
            SmokeFrameKind::Metadata => {
                if saw_metadata || saw_batch || completed {
                    return Err(protocol_error(
                        "RPC metadata must be the first result-stream frame",
                    ));
                }
                saw_metadata = true;
            }
            SmokeFrameKind::Batch => {
                if !saw_metadata {
                    return Err(protocol_error("RPC metadata must precede RPC batch frames"));
                }
                if completed {
                    return Err(protocol_error("RPC batch must not follow completion"));
                }
                if frame.payload_len == 0 {
                    return Err(protocol_error("RPC batch requires a non-empty payload"));
                }
                saw_batch = true;
            }
            SmokeFrameKind::Completion => {
                if !saw_metadata || !saw_batch {
                    return Err(protocol_error(
                        "RPC completion requires prior metadata and batch frames",
                    ));
                }
                if completed {
                    return Err(protocol_error("RPC completion must appear once"));
                }
                completed = true;
            }
        }
    }

    if saw_metadata && saw_batch && completed {
        Ok(())
    } else {
        Err(protocol_error("result-stream sequence is incomplete"))
    }
}

fn validate_completion_error_structured_contract() -> AndromedaResult<String> {
    let contract = inventory_reserve_stock_contract()?;
    let request = InvocationRequest {
        invocation_id: InvocationId::new(19),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    let structured_parameter_count = request.structured_parameters.len();

    let procedure = LocalProcedure {
        contract: contract.as_ref(),
        required_permissions: contract.required_permissions.clone(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            column_count: contract.result_streams[0].columns.len() as u32,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"protocol smoke mutation".to_vec(),
        rows_affected: 1,
    };

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let context = InvocationContext::new(TraceId::new(19), contract.required_permissions.clone());
    let outcome = runtime.execute_authorized(request, &procedure, &context)?;

    if outcome.completion.status != CompletionStatus::Committed {
        return Err(protocol_error(format!(
            "unexpected completion status: {:?}",
            outcome.completion.status
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
        outcome.completion.status,
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

fn protocol_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_smoke_report_is_deterministic_and_concise() {
        let first = protocol_smoke_report().unwrap();
        let second = protocol_smoke_report().unwrap();

        assert_eq!(first, second);
        assert!(first.contains("payload/frame lockstep: ok (9 codes)"));
        assert!(first.contains("result stream sequence: ok"));
        assert!(first.contains("telemetry datagram policy: ok"));
        assert!(first.contains("network sockets: not opened"));
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
