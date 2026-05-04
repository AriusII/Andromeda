#![forbid(unsafe_code)]

use andromeda_catalog::{
    CatalogSnapshot, INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID,
    inventory_domain_definition_batch, inventory_reserve_stock_contract,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, HardwareProfile, InvocationId, RequestId,
    SessionId,
};
use andromeda_exec::{
    CompletionStatus, InventoryReserveStockExecutor, InventoryStock, InvocationContext,
    InvocationRequest, LocalVerticalRuntime, ReserveStockCommand, V0InventoryRecoverableRuntime,
    V0InventoryReserveStockRpcPayload, encode_inventory_reserve_stock_v0_execute_frame,
    inventory_reserve_stock_v0_pdf_srpl_source,
};
use andromeda_observe::TraceId;
use andromeda_storage::{
    DatabaseManifest, FileWal, FileWalRecoveryReportV0, InMemoryWal, Lsn, StartupMode,
    report_file_wal_recovery_v0,
};
use std::path::{Path, PathBuf};

const PROTO_PAYLOAD_SOURCE: &str = include_str!("../../andromeda-proto/src/payload.rs");
const QUIC_FRAME_SOURCE: &str = include_str!("../../andromeda-quic/src/frame.rs");
const DEFAULT_V0_WAL_FILE: &str = "andromeda-v0-vertical.wal";

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
    match args.get(1).map(String::as_str) {
        Some("vertical") => return run_vertical_demo(),
        Some("vertical-v0") => {
            return run_vertical_v0_demo(vertical_v0_wal_path_from_args(&args[2..])?);
        }
        Some("protocol-smoke") => {
            print!(
                "{}",
                protocol_smoke_report(args[2..].iter().any(|arg| arg == "--detail"))?
            );
            return Ok(());
        }
        Some("recovery-inspect") => return run_recovery_inspect(&args[2..]),
        Some("-h" | "--help" | "help") | None => {}
        Some(command) => {
            return Err(cli_error(format!(
                "unknown command `{command}`; run `andromeda-cli --help`"
            )));
        }
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
    println!(
        "run `andromeda-cli vertical-v0 [--wal <path>]` for the recoverable V0 vertical prototype"
    );
    println!("run `andromeda-cli recovery-inspect <wal-path>` for a V0 FileWal recovery report");
    println!(
        "run `andromeda-cli protocol-smoke [--detail]` for local protocol contract inspection"
    );
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
    let effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 3,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 1,
        },
    )?;
    let procedure = effect.to_local_procedure(&contract)?;
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
    println!("remaining stock: {}", effect.result.remaining_quantity);
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

fn run_vertical_v0_demo(wal_path: PathBuf) -> AndromedaResult<()> {
    let contract = inventory_reserve_stock_contract()?;
    let catalog = inventory_catalog_snapshot()?;
    let request = InvocationRequest {
        invocation_id: InvocationId::new(2),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    std::fs::remove_file(&wal_path).ok();
    let encoded_frame = encode_inventory_reserve_stock_v0_execute_frame(
        RequestId::new(2),
        SessionId::new(2),
        V0InventoryReserveStockRpcPayload::new(42, 3)?,
    )?;
    let mut runtime = V0InventoryRecoverableRuntime::new(FileWal::open(&wal_path)?);
    let context = InvocationContext::new(TraceId::new(2), contract.required_permissions.clone());
    let outcome = runtime.execute_encoded_inventory_reserve_stock(
        &encoded_frame,
        inventory_reserve_stock_v0_pdf_srpl_source(),
        &catalog,
        &contract,
        request,
        &context,
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 1,
        },
    )?;
    drop(runtime);

    let report = recovery_report_for_path(&wal_path, Lsn::new(1))?;
    let replay_lsns = report
        .replay_lsns()
        .map(|lsn| lsn.get())
        .collect::<Vec<_>>();

    println!("Andromeda V0 recoverable vertical prototype");
    println!("procedure: {}", contract.object.name.as_catalog_path());
    println!("status: {:?}", outcome.vertical.completion.status);
    println!(
        "rows affected: {:?}",
        outcome.vertical.completion.rows_affected
    );
    println!(
        "remaining stock: {}",
        outcome.effect.result.remaining_quantity
    );
    println!(
        "durable WAL LSN: {}",
        outcome.durable_lsn().unwrap_or_else(|| Lsn::new(0)).get()
    );
    println!("WAL path: {}", wal_path.display());
    println!("recovery replay LSNs: {:?}", replay_lsns);
    println!("recovery boundary: {:?}", report.boundary_kind);
    println!("forensic required: {}", report.forensic_required);
    println!("result frames: {}", outcome.result_frames.len());

    debug_assert_eq!(
        outcome.vertical.completion.status,
        CompletionStatus::Committed
    );
    Ok(())
}

fn run_recovery_inspect(args: &[String]) -> AndromedaResult<()> {
    let options = recovery_inspect_options_from_args(args)?;
    let report = recovery_report_for_path(&options.wal_path, options.required_wal_start_lsn)?;
    print!("{}", recovery_inspect_report(&options.wal_path, &report));
    Ok(())
}

fn recovery_report_for_path(
    wal_path: &Path,
    required_wal_start_lsn: Lsn,
) -> AndromedaResult<FileWalRecoveryReportV0> {
    let manifest = v0_demo_manifest(required_wal_start_lsn);
    report_file_wal_recovery_v0(&manifest, StartupMode::SafeStart, wal_path)
}

fn recovery_inspect_report(wal_path: &Path, report: &FileWalRecoveryReportV0) -> String {
    let replay_lsns = report
        .replay_lsns()
        .map(|lsn| lsn.get())
        .collect::<Vec<_>>();
    let ignored_transactions = report
        .ignored_transaction_ids()
        .map(|id| id.get())
        .collect::<Vec<_>>();
    let scan_stop = report
        .scan_stop
        .map(|stop| format!("{:?} at byte {}", stop.reason, stop.offset))
        .unwrap_or_else(|| "none".to_string());

    let mut output = String::new();
    output.push_str("Andromeda V0 WAL recovery inspect\n");
    output.push_str(&format!("WAL path: {}\n", wal_path.display()));
    output.push_str(&format!("startup mode: {:?}\n", report.startup_mode));
    output.push_str(&format!(
        "format version: {}\n",
        report.header.format_version
    ));
    output.push_str("byte order: little-endian\n");
    output.push_str(&format!("segment id: {}\n", report.header.segment_id));
    output.push_str(&format!(
        "physical WAL bytes: {}\n",
        report.physical_wal_bytes
    ));
    output.push_str(&format!("scanned bytes: {}\n", report.scanned_bytes));
    output.push_str(&format!(
        "durable prefix bytes: {}\n",
        report.durable_prefix_bytes
    ));
    output.push_str(&format!(
        "durable prefix records: {}\n",
        report.durable_prefix_record_count
    ));
    output.push_str(&format!("durable LSN: {}\n", report.durable_lsn.get()));
    output.push_str(&format!("boundary: {:?}\n", report.boundary_kind));
    output.push_str(&format!("scan stop: {scan_stop}\n"));
    output.push_str(&format!(
        "forensic required: {}\n",
        report.forensic_required
    ));
    output.push_str(&format!("replay LSNs: {:?}\n", replay_lsns));
    output.push_str(&format!(
        "ignored transactions: {:?}\n",
        ignored_transactions
    ));
    output.push_str(&format!(
        "ignored records: {}\n",
        report.ignored_record_count
    ));
    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecoveryInspectOptions {
    wal_path: PathBuf,
    required_wal_start_lsn: Lsn,
}

fn vertical_v0_wal_path_from_args(args: &[String]) -> AndromedaResult<PathBuf> {
    let mut wal_path = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--wal" => {
                index += 1;
                let Some(path) = args.get(index) else {
                    return Err(cli_error("missing path after --wal"));
                };
                wal_path = Some(PathBuf::from(path));
            }
            unknown => {
                return Err(cli_error(format!(
                    "unknown vertical-v0 option `{unknown}`; expected `--wal <path>`"
                )));
            }
        }
        index += 1;
    }

    Ok(wal_path.unwrap_or_else(default_v0_wal_path))
}

fn recovery_inspect_options_from_args(args: &[String]) -> AndromedaResult<RecoveryInspectOptions> {
    let mut wal_path = None;
    let mut required_wal_start_lsn = Lsn::new(1);
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--required-wal-start-lsn" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(cli_error("missing value after --required-wal-start-lsn"));
                };
                required_wal_start_lsn =
                    Lsn::new(parse_u64_option(value, "--required-wal-start-lsn")?);
            }
            option if option.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown recovery-inspect option `{option}`"
                )));
            }
            path => {
                if wal_path.is_some() {
                    return Err(cli_error("recovery-inspect accepts exactly one WAL path"));
                }
                wal_path = Some(PathBuf::from(path));
            }
        }
        index += 1;
    }

    let Some(wal_path) = wal_path else {
        return Err(cli_error(
            "usage: andromeda-cli recovery-inspect <wal-path> [--required-wal-start-lsn <lsn>]",
        ));
    };

    Ok(RecoveryInspectOptions {
        wal_path,
        required_wal_start_lsn,
    })
}

fn parse_u64_option(value: &str, option: &str) -> AndromedaResult<u64> {
    value
        .parse::<u64>()
        .map_err(|_| cli_error(format!("{option} expects an unsigned integer")))
}

fn default_v0_wal_path() -> PathBuf {
    std::env::temp_dir().join(DEFAULT_V0_WAL_FILE)
}

fn v0_demo_manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 1,
    }
}

fn inventory_catalog_snapshot() -> AndromedaResult<CatalogSnapshot> {
    let batch = inventory_domain_definition_batch()?;
    let plan = batch.dry_run()?;
    let mut snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        batch.base_version,
    );
    snapshot.apply_mutation_plan(&plan.mutation_plan)?;
    Ok(snapshot)
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

fn protocol_smoke_report(detailed: bool) -> AndromedaResult<String> {
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
    if detailed {
        report.push_str("payload/frame codes:\n");
        for expected in PROTOCOL_CODE_LOCKSTEP {
            report.push_str(&format!(
                "  - {}: payload={}, frame={}, code={}\n",
                expected.name, expected.payload_const, expected.frame_const, expected.code
            ));
        }
        report.push_str("representative result frames:\n");
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

fn cli_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_smoke_report_is_deterministic_and_concise() {
        let first = protocol_smoke_report(false).unwrap();
        let second = protocol_smoke_report(false).unwrap();

        assert_eq!(first, second);
        assert!(first.contains("payload/frame lockstep: ok (9 codes)"));
        assert!(first.contains("result stream sequence: ok"));
        assert!(first.contains("telemetry datagram policy: ok"));
        assert!(first.contains("network sockets: not opened"));
    }

    #[test]
    fn protocol_smoke_detail_lists_stable_codes() {
        let report = protocol_smoke_report(true).unwrap();

        assert!(report.contains("payload/frame codes:"));
        assert!(report.contains("RpcExecuteRequest: payload=RPC_EXECUTE_REQUEST_WIRE_CODE"));
        assert!(report.contains("representative result frames:"));
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

    #[test]
    fn vertical_v0_defaults_to_temp_wal_and_accepts_explicit_path() {
        let explicit = vertical_v0_wal_path_from_args(&[
            "--wal".to_string(),
            "target/andromeda-cli-test.wal".to_string(),
        ])
        .unwrap();

        assert_eq!(explicit, PathBuf::from("target/andromeda-cli-test.wal"));
        assert!(
            vertical_v0_wal_path_from_args(&[])
                .unwrap()
                .ends_with(DEFAULT_V0_WAL_FILE)
        );
    }

    #[test]
    fn recovery_inspect_requires_one_path() {
        let options = recovery_inspect_options_from_args(&[
            "target/andromeda-cli-test.wal".to_string(),
            "--required-wal-start-lsn".to_string(),
            "2".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.wal_path,
            PathBuf::from("target/andromeda-cli-test.wal")
        );
        assert_eq!(options.required_wal_start_lsn, Lsn::new(2));
        assert_eq!(
            recovery_inspect_options_from_args(&[]).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
