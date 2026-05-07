use super::support::{
    assert_contains_all, assert_dispatch_success, assert_success, run_cli, stdout,
};

#[test]
fn benchmark_help_command_executes() {
    assert_dispatch_success(["benchmark", "--help"]);
}

#[test]
fn benchmark_workloads_command_executes() {
    assert_dispatch_success(["benchmark", "workloads"]);
}

#[test]
fn benchmark_contract_accepts_diagnostic_json() {
    assert_dispatch_success(["benchmark", "contract", "--diagnostic-json"]);
}

#[test]
fn benchmark_run_executes_bounded_smoke_runner() {
    assert_dispatch_success([
        "benchmark",
        "run",
        "vertical-v0-smoke",
        "--duration-ms",
        "1000",
        "--samples",
        "1",
    ]);
}

#[test]
fn benchmark_run_diagnostic_json_is_diagnostic_only() {
    let output = run_cli([
        "benchmark",
        "run",
        "protocol-smoke-contract",
        "--duration-ms",
        "1000",
        "--samples",
        "5",
        "--diagnostic-json",
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert!(json.trim_start().starts_with('{'));
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.benchmark.run.v1\"",
            "\"diagnostic_only\":true",
            "\"runner\":\"deterministic-smoke\"",
            "\"workload_id\":\"protocol-smoke-contract\"",
            "\"budget_status\":\"passed\"",
        ],
    );
}
