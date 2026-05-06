#![forbid(unsafe_code)]

use std::process::{Command, Output};

#[test]
fn benchmark_workloads_diagnostic_json_lists_btree_node_codec_smoke() {
    let output = run_cli(["benchmark", "workloads", "--diagnostic-json"]);

    assert_success(&output);
    let json = stdout(&output);
    assert!(json.trim_start().starts_with('{'));
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.benchmark.workloads.v1\"",
            "\"diagnostic_only\":true",
            "\"id\":\"btree-node-codec-smoke\"",
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
        "--diagnostic-json",
    ]);

    assert_success(&output);
    let json = stdout(&output);
    assert!(json.trim_start().starts_with('{'));
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.benchmark.run.v1\"",
            "\"workload_id\":\"btree-node-codec-smoke\"",
            "\"measurement_mode\":\"harness-diagnostic\"",
            "\"latency_source\":\"storage-btree-node-v1-codec\"",
            "\"engine_harness\":\"BTreeNodeV1::encode+decode\"",
            "\"synthetic_model_version\":null",
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
    ]);

    assert_success(&output);
    let human = stdout(&output);
    assert!(!human.trim_start().starts_with('{'));
    assert_contains_all(
        &human,
        &[
            "workload id: btree-node-codec-smoke",
            "measurement mode: harness-diagnostic",
            "latency source: storage-btree-node-v1-codec",
            "engine harness: BTreeNodeV1::encode+decode",
            "synthetic model version: none",
        ],
    );
}

fn run_cli<const N: usize>(args: [&str; N]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
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
