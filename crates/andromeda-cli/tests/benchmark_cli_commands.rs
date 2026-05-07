#![forbid(unsafe_code)]

use andromeda_bench::{DEFAULT_TEMP_BYTES, MAX_TEMP_BYTES};
use andromeda_cli::cmd::dispatch_command;
use std::process::{Command, Output};

#[test]
fn benchmark_help_uses_native_diagnostic_surface_wording() {
    let output = run_cli(["benchmark", "--help"]);

    assert_success(&output);
    let human = stdout(&output);
    assert!(!human.trim_start().starts_with('{'));
    assert_no_human_surface_drift(&human);
    assert_contains_all(
        &human,
        &[
            "Andromeda benchmark administration commands",
            "Run a deterministic bounded diagnostic smoke workload",
            "--diagnostic-json   Emit diagnostic machine-readable JSON output",
        ],
    );
}

#[test]
fn benchmark_workloads_diagnostic_json_lists_btree_node_codec_smoke() {
    let output = run_cli(["benchmark", "workloads", "--diagnostic-json"]);

    assert_success(&output);
    let json = stdout(&output);
    assert!(json.trim_start().starts_with('{'));
    assert_diagnostic_json_not_runtime_protocol(&json);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.benchmark.workloads.v1\"",
            "\"diagnostic_only\":true",
            "\"id\":\"btree-node-codec-smoke\"",
            "\"max_temp_bytes\":67108864",
            "B-Tree durable node V1 encode/decode over deterministic page images",
        ],
    );
}

#[test]
fn benchmark_run_btree_node_codec_json_exposes_runtime_harness_metadata() {
    let output = run_cli([
        "benchmark",
        "run",
        "btree-node-codec-smoke",
        "--duration-ms",
        "1000",
        "--samples",
        "2",
        "--warmups",
        "0",
        "--temp-budget-bytes",
        "1048576",
        "--diagnostic-json",
    ]);

    assert_success(&output);
    let json = stdout(&output);
    assert!(json.trim_start().starts_with('{'));
    assert_diagnostic_json_not_runtime_protocol(&json);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.benchmark.run.v1\"",
            "\"workload_id\":\"btree-node-codec-smoke\"",
            "\"temp_budget_bytes\":1048576",
            "\"measurement_mode\":\"harness-diagnostic\"",
            "\"latency_source\":\"storage-btree-node-v1-codec\"",
            "\"engine_harness\":\"BTreeNodeV1::encode+decode\"",
            "\"synthetic_model_version\":null",
            "\"authoritative\":false",
            "\"can_select_plan_alone\":false",
            "\"optimizer_boundary\":\"advisory-only\"",
        ],
    );
}

#[test]
fn benchmark_run_btree_node_codec_human_output_exposes_runtime_harness_metadata() {
    let output = run_cli([
        "benchmark",
        "run",
        "btree-node-codec-smoke",
        "--duration-ms",
        "1000",
        "--samples",
        "2",
        "--warmups",
        "0",
        "--temp-budget-bytes",
        "1048576",
    ]);

    assert_success(&output);
    let human = stdout(&output);
    assert!(!human.trim_start().starts_with('{'));
    assert_no_human_surface_drift(&human);
    assert_contains_all(
        &human,
        &[
            "workload id: btree-node-codec-smoke",
            "temp budget bytes: 1048576",
            "measurement mode: harness-diagnostic",
            "latency source: storage-btree-node-v1-codec",
            "engine harness: BTreeNodeV1::encode+decode",
            "synthetic model version: none",
            "authoritative: false",
            "can select plan alone: false",
            "optimizer boundary: advisory-only",
        ],
    );
}

#[test]
fn benchmark_run_zero_temp_budget_message_is_bounded() {
    let err = dispatch_benchmark_run(["run", "vertical-v0-smoke", "--temp-budget-bytes", "0"]);

    assert_eq!(
        err.message(),
        "--temp-budget-bytes must be greater than zero"
    );
}

#[test]
fn benchmark_run_global_temp_budget_message_is_bounded() {
    let temp_budget = (MAX_TEMP_BYTES + 1).to_string();
    let err = dispatch_benchmark_run([
        "run",
        "vertical-v0-smoke",
        "--temp-budget-bytes",
        temp_budget.as_str(),
    ]);

    assert_eq!(
        err.message(),
        format!("--temp-budget-bytes must be <= {MAX_TEMP_BYTES}")
    );
    assert!(!err.message().contains(&temp_budget));
}

#[test]
fn benchmark_run_workload_temp_budget_message_is_bounded() {
    let temp_budget = (DEFAULT_TEMP_BYTES + 1).to_string();
    let err = dispatch_benchmark_run([
        "run",
        "protocol-smoke-contract",
        "--temp-budget-bytes",
        temp_budget.as_str(),
    ]);

    assert_eq!(
        err.message(),
        format!(
            "workload `protocol-smoke-contract` temp budget must be <= {DEFAULT_TEMP_BYTES} bytes"
        )
    );
    assert!(!err.message().contains(&temp_budget));
}

#[test]
fn benchmark_contract_human_output_marks_json_as_diagnostic() {
    let output = run_cli(["benchmark", "contract"]);

    assert_success(&output);
    let human = stdout(&output);
    assert!(!human.trim_start().starts_with('{'));
    assert_no_human_surface_drift(&human);
    assert_contains_all(
        &human,
        &[
            "surface: administration diagnostics only",
            "diagnostic JSON: --diagnostic-json only; not runtime protocol",
            "optimizer boundary: advisory-only",
        ],
    );
}

#[test]
fn benchmark_contract_json_exposes_advisory_policy() {
    let output = run_cli(["benchmark", "contract", "--diagnostic-json"]);

    assert_success(&output);
    let json = stdout(&output);
    assert_diagnostic_json_not_runtime_protocol(&json);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.benchmark.contract.v1\"",
            "\"optimizer_use\":{\"authoritative\":false",
            "\"can_select_plan_alone\":false",
            "\"boundary\":\"advisory-only\"",
            "\"max_samples\":100",
            "\"max_temp_bytes\":67108864",
        ],
    );
}

#[test]
fn benchmark_crud_scenarios_json_exposes_limits_and_advisory_policy() {
    let output = run_cli(["benchmark", "crud-scenarios", "--diagnostic-json"]);

    assert_success(&output);
    let json = stdout(&output);
    assert_diagnostic_json_not_runtime_protocol(&json);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.benchmark.crud_scenarios.v1\"",
            "\"diagnostic_only\":true",
            "\"optimizer_use\":{\"authoritative\":false",
            "\"can_select_plan_alone\":false",
            "\"boundary\":\"advisory-only\"",
            "\"global_limits\":{\"max_threads\":8",
            "\"max_batch_size\":1000000",
            "\"max_row_count\":1000000",
            "\"max_duration_ms\":60000",
            "\"id\":\"crud-single-1\"",
        ],
    );
}

#[test]
fn benchmark_crud_run_json_exposes_advisory_boundary() {
    let output = run_cli([
        "benchmark",
        "crud",
        "crud-single-1",
        "--seed",
        "42",
        "--diagnostic-json",
    ]);

    assert_success(&output);
    let json = stdout(&output);
    assert_diagnostic_json_not_runtime_protocol(&json);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.bench.crud.result.v1\"",
            "\"diagnostic_only\":true",
            "\"authoritative\":false",
            "\"can_select_plan_alone\":false",
            "\"optimizer_boundary\":\"advisory-only\"",
            "\"scenario_id\":\"crud-single-1\"",
            "\"seed\":42",
        ],
    );
}

#[test]
fn benchmark_crud_run_human_output_exposes_advisory_boundary() {
    let output = run_cli(["benchmark", "crud", "crud-single-1", "--seed", "42"]);

    assert_success(&output);
    let human = stdout(&output);
    assert!(!human.trim_start().starts_with('{'));
    assert_no_human_surface_drift(&human);
    assert_contains_all(
        &human,
        &[
            "scenario_id: crud-single-1",
            "diagnostic_only: true",
            "authoritative: false",
            "can select plan alone: false",
            "optimizer boundary: advisory-only",
            "seed: 42",
        ],
    );
}

fn run_cli<const N: usize>(args: [&str; N]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

fn dispatch_benchmark_run<const N: usize>(args: [&str; N]) -> andromeda_core::AndromedaError {
    let mut command_args = Vec::with_capacity(N + 1);
    command_args.push("benchmark".to_string());
    command_args.extend(args.into_iter().map(str::to_string));
    dispatch_command(&command_args).expect_err("benchmark run should reject invalid request")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_contains_all(text: &str, expected: &[&str]) {
    for item in expected {
        assert!(text.contains(item), "expected `{item}` in `{text}`");
    }
}

fn assert_diagnostic_json_not_runtime_protocol(text: &str) {
    assert!(
        text.contains("\"diagnostic_only\":true") || text.contains("\"authoritative\":false"),
        "diagnostic JSON must expose diagnostic/advisory status: {text}"
    );
    let lower = text.to_ascii_lowercase();
    for token in [
        "application/json",
        "jsonrpc",
        "default json",
        "json runtime",
        "wire semantics",
    ] {
        assert!(
            !lower.contains(token),
            "diagnostic JSON must not advertise `{token}` as an Andromeda runtime protocol: {text}"
        );
    }
}

fn assert_no_human_surface_drift(text: &str) {
    let lower = text.to_ascii_lowercase();
    for token in [
        "--sql",
        "ad hoc sql",
        "execute sql",
        "raw sql",
        "result set",
        "resultset",
        "trace query",
        "grpc",
        "tonic",
        "application/json",
        "jsonrpc",
        "default json",
        "json runtime",
    ] {
        assert!(
            !lower.contains(token),
            "benchmark human output must not advertise `{token}` as an Andromeda runtime surface: {text}"
        );
    }

    for token in ["query", "queries"] {
        assert!(
            !lower
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .any(|word| word == token),
            "benchmark human output must not advertise `{token}` as native user wording: {text}"
        );
    }
}
