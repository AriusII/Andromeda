use crate::diagnostic_json::json_string;
use andromeda_bench::{
    BenchmarkEvidence, BenchmarkHardwareProfile, BudgetStatus, DEFAULT_DURATION_MS,
    DEFAULT_SAMPLES, DEFAULT_WARMUPS, MAX_DURATION_MS, MAX_SAMPLES, MAX_WARMUPS, WORKLOADS,
};
use std::fmt::Write as _;

pub(super) fn print_benchmark_run_evidence(evidence: &BenchmarkEvidence, diagnostic_json: bool) {
    print!(
        "{}",
        format_benchmark_run_evidence(evidence, diagnostic_json)
    );
}

pub(super) fn format_benchmark_run_evidence(
    evidence: &BenchmarkEvidence,
    diagnostic_json: bool,
) -> String {
    if diagnostic_json {
        return format_benchmark_run_json(evidence);
    }

    let mut output = String::new();
    writeln!(output, "Andromeda benchmark diagnostic evidence").expect("format benchmark output");
    writeln!(output, "=======================================").expect("format benchmark output");
    writeln!(output, "runner: deterministic-smoke").expect("format benchmark output");
    writeln!(output, "workload id: {}", evidence.workload_id).expect("format benchmark output");
    writeln!(
        output,
        "hardware profile: {}",
        evidence.hardware_profile.as_str()
    )
    .expect("format benchmark output");
    writeln!(output, "duration ms: {}", evidence.duration_ms).expect("format benchmark output");
    writeln!(output, "samples: {}", evidence.samples).expect("format benchmark output");
    writeln!(output, "warmups: {}", evidence.warmups).expect("format benchmark output");
    writeln!(
        output,
        "started at unix ms: {}",
        evidence.started_at_unix_ms
    )
    .expect("format benchmark output");
    writeln!(output, "elapsed ms: {}", evidence.elapsed_ms).expect("format benchmark output");
    writeln!(output, "sample count: {}", evidence.sample_count).expect("format benchmark output");
    writeln!(output, "p50 latency us: {}", evidence.p50_latency_us)
        .expect("format benchmark output");
    writeln!(output, "p95 latency us: {}", evidence.p95_latency_us)
        .expect("format benchmark output");
    writeln!(output, "error count: {}", evidence.error_count).expect("format benchmark output");
    writeln!(
        output,
        "budget status: {}",
        budget_status_str(evidence.budget_status)
    )
    .expect("format benchmark output");
    writeln!(output, "diagnostic only: {}", evidence.diagnostic_only)
        .expect("format benchmark output");
    writeln!(
        output,
        "measurement mode: {}",
        evidence.measurement_mode.as_str()
    )
    .expect("format benchmark output");
    writeln!(output, "latency source: {}", evidence.latency_source)
        .expect("format benchmark output");
    writeln!(
        output,
        "engine harness: {}",
        evidence.engine_harness.unwrap_or("none")
    )
    .expect("format benchmark output");
    writeln!(
        output,
        "synthetic model version: {}",
        evidence.synthetic_model_version.unwrap_or("none")
    )
    .expect("format benchmark output");
    output
}

fn format_benchmark_run_json(evidence: &BenchmarkEvidence) -> String {
    format!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"runner\":{},\"evidence\":{{\"workload_id\":{},\"hardware_profile\":{},\"duration_ms\":{},\"samples\":{},\"warmups\":{},\"started_at_unix_ms\":{},\"elapsed_ms\":{},\"sample_count\":{},\"p50_latency_us\":{},\"p95_latency_us\":{},\"error_count\":{},\"budget_status\":{},\"diagnostic_only\":{},\"measurement_mode\":{},\"latency_source\":{},\"engine_harness\":{},\"synthetic_model_version\":{}}}}}",
        json_string("andromeda.cli.benchmark.run.v1"),
        json_string("deterministic-smoke"),
        json_string(&evidence.workload_id),
        json_string(evidence.hardware_profile.as_str()),
        evidence.duration_ms,
        evidence.samples,
        evidence.warmups,
        evidence.started_at_unix_ms,
        evidence.elapsed_ms,
        evidence.sample_count,
        evidence.p50_latency_us,
        evidence.p95_latency_us,
        evidence.error_count,
        json_string(budget_status_str(evidence.budget_status)),
        evidence.diagnostic_only,
        json_string(evidence.measurement_mode.as_str()),
        json_string(evidence.latency_source),
        optional_json_string(evidence.engine_harness),
        optional_json_string(evidence.synthetic_model_version)
    ) + "\n"
}

fn optional_json_string(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

fn budget_status_str(status: BudgetStatus) -> &'static str {
    match status {
        BudgetStatus::Passed => "passed",
        BudgetStatus::Failed => "failed",
    }
}

pub(super) fn print_benchmark_help() {
    println!("Andromeda benchmark administration commands");
    println!();
    println!("USAGE: andromeda-cli benchmark <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  workloads           List allowed bounded benchmark workloads");
    println!("  contract            Display benchmark evidence contract and limits");
    println!("  run <workload>      Run a deterministic bounded diagnostic smoke workload");
    println!("  crud-scenarios      List available CRUD benchmark scenarios");
    println!("  crud <scenario>     Run a CRUD workload scenario");
    println!();
    println!("RUN OPTIONS:");
    println!("  --duration-ms <ms>  Duration cap; global max {MAX_DURATION_MS} ms");
    println!("  --samples <n>       Sample cap; global max {MAX_SAMPLES}");
    println!("  --warmups <n>       Warmup cap; global max {MAX_WARMUPS}");
    println!("  --hardware-profile <conservative|declared-local>");
    println!("  --diagnostic-json   Emit diagnostic machine-readable JSON output");
    println!();
    println!("CRUD OPTIONS:");
    println!("  --seed <u64>        PRNG seed for deterministic data generation (default: 42)");
    println!("  --diagnostic-json   Emit diagnostic machine-readable JSON output");
    println!("  -h, --help          Show this help message");
}

pub(super) fn print_workloads(diagnostic_json: bool) {
    print!("{}", format_workloads(diagnostic_json));
}

pub(super) fn format_workloads(diagnostic_json: bool) -> String {
    if diagnostic_json {
        return format_workloads_json();
    }

    let mut output = String::new();
    writeln!(output, "Andromeda benchmark workloads").expect("format benchmark output");
    writeln!(output, "=============================").expect("format benchmark output");
    for workload in WORKLOADS {
        writeln!(output, "- {}", workload.id).expect("format benchmark output");
        writeln!(
            output,
            "  description: {}",
            cli_workload_description(workload.id, workload.description)
        )
        .expect("format benchmark output");
        writeln!(output, "  max duration ms: {}", workload.max_duration_ms)
            .expect("format benchmark output");
        writeln!(output, "  max samples: {}", workload.max_samples)
            .expect("format benchmark output");
        writeln!(
            output,
            "  budget p50 us: {}",
            workload.budget.max_p50_latency_us
        )
        .expect("format benchmark output");
        writeln!(
            output,
            "  budget p95 us: {}",
            workload.budget.max_p95_latency_us
        )
        .expect("format benchmark output");
        writeln!(
            output,
            "  budget error ppm: {}",
            workload.budget.max_error_rate_ppm
        )
        .expect("format benchmark output");
    }
    output
}

pub(super) fn print_benchmark_contract(diagnostic_json: bool) {
    let profile = BenchmarkHardwareProfile::Conservative.materialize();

    if diagnostic_json {
        let architecture = json_string(&format!("{:?}", profile.architecture));
        println!(
            "{{\"schema\":{},\"diagnostic_only\":true,\"global_limits\":{{\"max_duration_ms\":{MAX_DURATION_MS},\"max_samples\":{MAX_SAMPLES},\"max_warmups\":{MAX_WARMUPS}}},\"default_limits\":{{\"duration_ms\":{DEFAULT_DURATION_MS},\"samples\":{DEFAULT_SAMPLES},\"warmups\":{DEFAULT_WARMUPS}}},\"hardware_profile\":{{\"name\":{},\"architecture\":{},\"has_simd\":{},\"has_direct_io\":{},\"gpu_available\":{}}},\"exit_codes\":{{\"success\":0,\"validation_failure\":1}},\"evidence_fields\":[\"workload_id\",\"hardware_profile\",\"duration_ms\",\"samples\",\"warmups\",\"started_at_unix_ms\",\"elapsed_ms\",\"sample_count\",\"p50_latency_us\",\"p95_latency_us\",\"error_count\",\"budget_status\",\"diagnostic_only\",\"measurement_mode\",\"latency_source\",\"engine_harness\",\"synthetic_model_version\"]}}",
            json_string("andromeda.cli.benchmark.contract.v1"),
            json_string("conservative"),
            architecture,
            profile.has_simd,
            profile.has_direct_io,
            profile.gpu.available
        );
        return;
    }

    println!("Andromeda benchmark contract");
    println!("============================");
    println!("surface: administration diagnostics only");
    println!("JSON: --diagnostic-json only; not runtime wire");
    println!("global max duration ms: {MAX_DURATION_MS}");
    println!("global max samples: {MAX_SAMPLES}");
    println!("global max warmups: {MAX_WARMUPS}");
    println!("default duration ms: {DEFAULT_DURATION_MS}");
    println!("default samples: {DEFAULT_SAMPLES}");
    println!("default warmups: {DEFAULT_WARMUPS}");
    println!(
        "hardware profile: conservative ({:?})",
        profile.architecture
    );
    println!("exit code 0: command/validation success");
    println!("exit code 1: validation failure");
    println!(
        "required evidence: workload_id, hardware_profile, duration_ms, samples, warmups, started_at_unix_ms, elapsed_ms, sample_count, p50_latency_us, p95_latency_us, error_count, budget_status, diagnostic_only, measurement_mode, latency_source, engine_harness, synthetic_model_version"
    );
}

fn format_workloads_json() -> String {
    let mut output = format!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"workloads\":[",
        json_string("andromeda.cli.benchmark.workloads.v1")
    );
    for (index, workload) in WORKLOADS.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        write!(
            output,
            "{{\"id\":{},\"description\":{},\"max_duration_ms\":{},\"max_samples\":{},\"budget\":{{\"max_p50_latency_us\":{},\"max_p95_latency_us\":{},\"max_error_rate_ppm\":{}}}}}",
            json_string(workload.id),
            json_string(cli_workload_description(workload.id, workload.description)),
            workload.max_duration_ms,
            workload.max_samples,
            workload.budget.max_p50_latency_us,
            workload.budget.max_p95_latency_us,
            workload.budget.max_error_rate_ppm
        )
        .expect("format benchmark output");
    }
    output.push_str("]}}\n");
    output
}

fn cli_workload_description<'a>(id: &str, description: &'a str) -> &'a str {
    if id == "btree-node-codec-smoke" {
        "harness-diagnostic B-Tree durable node V1 encode/decode over deterministic page images"
    } else {
        description
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_bench::{
        BTREE_NODE_CODEC_HARNESS_NAME, BTREE_NODE_CODEC_HARNESS_SOURCE,
        BTREE_NODE_CODEC_WORKLOAD_ID, BenchmarkRunRequest, run_bounded_benchmark,
    };

    #[test]
    fn workloads_json_lists_btree_node_codec_smoke() {
        let json = format_workloads(true);

        assert!(json.contains("\"schema\":\"andromeda.cli.benchmark.workloads.v1\""));
        assert!(json.contains("\"id\":\"btree-node-codec-smoke\""));
        assert!(
            json.contains("B-Tree durable node V1 encode/decode over deterministic page images")
        );
    }

    #[test]
    fn btree_node_codec_run_json_exposes_harness_evidence_fields() {
        let mut request = BenchmarkRunRequest::new(BTREE_NODE_CODEC_WORKLOAD_ID);
        request.samples = 2;
        request.warmups = 0;
        let evidence = run_bounded_benchmark(&request).unwrap();

        let json = format_benchmark_run_evidence(&evidence, true);

        assert!(json.contains("\"measurement_mode\":\"harness-diagnostic\""));
        assert!(json.contains(&format!(
            "\"latency_source\":\"{BTREE_NODE_CODEC_HARNESS_SOURCE}\""
        )));
        assert!(json.contains(&format!(
            "\"engine_harness\":\"{BTREE_NODE_CODEC_HARNESS_NAME}\""
        )));
        assert!(json.contains("\"synthetic_model_version\":null"));
    }

    #[test]
    fn btree_node_codec_human_output_exposes_harness_evidence_fields() {
        let mut request = BenchmarkRunRequest::new(BTREE_NODE_CODEC_WORKLOAD_ID);
        request.samples = 2;
        request.warmups = 0;
        let evidence = run_bounded_benchmark(&request).unwrap();

        let human = format_benchmark_run_evidence(&evidence, false);

        assert!(human.contains("measurement mode: harness-diagnostic"));
        assert!(human.contains(&format!(
            "latency source: {BTREE_NODE_CODEC_HARNESS_SOURCE}"
        )));
        assert!(human.contains(&format!("engine harness: {BTREE_NODE_CODEC_HARNESS_NAME}")));
        assert!(human.contains("synthetic model version: none"));
    }
}
