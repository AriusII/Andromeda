use crate::diagnostic_json::json_string;
use andromeda_bench_workload::{
    BenchmarkHardwareProfile, DEFAULT_DURATION_MS, DEFAULT_SAMPLES, DEFAULT_TEMP_BYTES,
    DEFAULT_WARMUPS, MAX_DURATION_MS, MAX_SAMPLES, MAX_TEMP_BYTES, MAX_WARMUPS, WORKLOADS,
};
use andromeda_scenario_evidence::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY, BenchmarkEvidence, BudgetStatus,
};
use std::fmt::Write as _;

macro_rules! benchmark_line {
    ($output:expr, $($arg:tt)*) => {
        writeln!($output, $($arg)*).expect("format benchmark output")
    };
}

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
    benchmark_line!(output, "Andromeda benchmark diagnostic evidence");
    benchmark_line!(output, "=======================================");
    benchmark_line!(output, "runner: deterministic-smoke");
    benchmark_line!(output, "workload id: {}", evidence.workload_id);
    benchmark_line!(output, "hypothesis: {}", evidence.workload_hypothesis);
    benchmark_line!(
        output,
        "workload shape version: {}",
        evidence.workload_shape_version
    );
    benchmark_line!(output, "workload size: {}", evidence.workload_size);
    benchmark_line!(output, "primary metric: {}", evidence.primary_metric);
    benchmark_line!(output, "baseline ref: {}", evidence.baseline_ref);
    benchmark_line!(output, "budget origin: {}", evidence.budget_origin);
    benchmark_line!(output, "decision linkage: {}", evidence.decision_linkage);
    benchmark_line!(
        output,
        "hardware profile: {}",
        evidence.hardware_profile.as_str()
    );
    benchmark_line!(output, "duration ms: {}", evidence.duration_ms);
    benchmark_line!(output, "samples: {}", evidence.samples);
    benchmark_line!(output, "warmups: {}", evidence.warmups);
    benchmark_line!(output, "temp budget bytes: {}", evidence.temp_budget_bytes);
    benchmark_line!(
        output,
        "started at unix ms: {}",
        evidence.started_at_unix_ms
    );
    benchmark_line!(output, "elapsed ms: {}", evidence.elapsed_ms);
    benchmark_line!(output, "sample count: {}", evidence.sample_count);
    benchmark_line!(output, "p50 latency us: {}", evidence.p50_latency_us);
    benchmark_line!(output, "p95 latency us: {}", evidence.p95_latency_us);
    benchmark_line!(output, "error count: {}", evidence.error_count);
    benchmark_line!(
        output,
        "budget status: {}",
        budget_status_str(evidence.budget_status)
    );
    benchmark_line!(output, "diagnostic only: {}", evidence.diagnostic_only);
    benchmark_line!(
        output,
        "measurement mode: {}",
        evidence.measurement_mode.as_str()
    );
    benchmark_line!(output, "latency source: {}", evidence.latency_source);
    benchmark_line!(
        output,
        "engine harness: {}",
        evidence.engine_harness.unwrap_or("none")
    );
    benchmark_line!(
        output,
        "synthetic model version: {}",
        evidence.synthetic_model_version.unwrap_or("none")
    );
    benchmark_line!(output, "authoritative: {}", evidence.is_authoritative());
    benchmark_line!(
        output,
        "can select plan alone: {}",
        evidence.can_select_plan_alone()
    );
    benchmark_line!(
        output,
        "optimizer boundary: {}",
        evidence.optimizer_consumption_role()
    );
    output
}

fn format_benchmark_run_json(evidence: &BenchmarkEvidence) -> String {
    format!(
        "{{\"schema\":{},\"diagnostic_only\":{},\"runner\":{},\"evidence\":{{\"workload_id\":{},\"workload_hypothesis\":{},\"workload_shape_version\":{},\"workload_size\":{},\"primary_metric\":{},\"baseline_ref\":{},\"budget_origin\":{},\"decision_linkage\":{},\"hardware_profile\":{},\"duration_ms\":{},\"samples\":{},\"warmups\":{},\"temp_budget_bytes\":{},\"started_at_unix_ms\":{},\"elapsed_ms\":{},\"sample_count\":{},\"p50_latency_us\":{},\"p95_latency_us\":{},\"error_count\":{},\"budget_status\":{},\"diagnostic_only\":{},\"measurement_mode\":{},\"latency_source\":{},\"engine_harness\":{},\"synthetic_model_version\":{},\"authoritative\":{},\"can_select_plan_alone\":{},\"optimizer_boundary\":{}}}}}",
        json_string("andromeda.cli.benchmark.run.v1"),
        evidence.diagnostic_only,
        json_string("deterministic-smoke"),
        json_string(&evidence.workload_id),
        json_string(evidence.workload_hypothesis),
        json_string(evidence.workload_shape_version),
        json_string(evidence.workload_size),
        json_string(evidence.primary_metric),
        json_string(evidence.baseline_ref),
        json_string(evidence.budget_origin),
        json_string(evidence.decision_linkage),
        json_string(evidence.hardware_profile.as_str()),
        evidence.duration_ms,
        evidence.samples,
        evidence.warmups,
        evidence.temp_budget_bytes,
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
        optional_json_string(evidence.synthetic_model_version),
        evidence.is_authoritative(),
        evidence.can_select_plan_alone(),
        json_string(evidence.optimizer_consumption_role())
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
    println!("  --temp-budget-bytes <bytes>  Temp budget cap; global max {MAX_TEMP_BYTES} bytes");
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
    benchmark_line!(output, "Andromeda benchmark workloads");
    benchmark_line!(output, "=============================");
    for workload in WORKLOADS {
        benchmark_line!(output, "- {}", workload.id);
        benchmark_line!(
            output,
            "  description: {}",
            cli_workload_description(workload.id, workload.description)
        );
        benchmark_line!(output, "  hypothesis: {}", workload.hypothesis);
        benchmark_line!(
            output,
            "  workload shape version: {}",
            workload.workload_shape_version
        );
        benchmark_line!(output, "  workload size: {}", workload.workload_size);
        benchmark_line!(output, "  primary metric: {}", workload.primary_metric);
        benchmark_line!(output, "  baseline ref: {}", workload.baseline_ref);
        benchmark_line!(output, "  budget origin: {}", workload.budget_origin);
        benchmark_line!(output, "  decision linkage: {}", workload.decision_linkage);
        benchmark_line!(output, "  max duration ms: {}", workload.max_duration_ms);
        benchmark_line!(output, "  max samples: {}", workload.max_samples);
        benchmark_line!(output, "  max temp bytes: {}", workload.max_temp_bytes);
        benchmark_line!(
            output,
            "  budget p50 us: {}",
            workload.budget.max_p50_latency_us
        );
        benchmark_line!(
            output,
            "  budget p95 us: {}",
            workload.budget.max_p95_latency_us
        );
        benchmark_line!(
            output,
            "  budget error ppm: {}",
            workload.budget.max_error_rate_ppm
        );
    }
    output
}

pub(super) fn print_benchmark_contract(diagnostic_json: bool) {
    let profile = BenchmarkHardwareProfile::Conservative.materialize();

    if diagnostic_json {
        let architecture = json_string(&format!("{:?}", profile.architecture));
        println!(
            "{{\"schema\":{},\"diagnostic_only\":true,\"global_limits\":{{\"max_duration_ms\":{MAX_DURATION_MS},\"max_samples\":{MAX_SAMPLES},\"max_warmups\":{MAX_WARMUPS},\"max_temp_bytes\":{MAX_TEMP_BYTES}}},\"default_limits\":{{\"duration_ms\":{DEFAULT_DURATION_MS},\"samples\":{DEFAULT_SAMPLES},\"warmups\":{DEFAULT_WARMUPS},\"temp_budget_bytes\":{DEFAULT_TEMP_BYTES}}},\"hardware_profile\":{{\"name\":{},\"architecture\":{},\"has_simd\":{},\"has_direct_io\":{},\"gpu_available\":{}}},\"optimizer_use\":{{\"authoritative\":{},\"can_select_plan_alone\":{},\"boundary\":{}}},\"exit_codes\":{{\"success\":0,\"validation_failure\":1}},\"evidence_fields\":[\"workload_id\",\"workload_hypothesis\",\"workload_shape_version\",\"workload_size\",\"primary_metric\",\"baseline_ref\",\"budget_origin\",\"decision_linkage\",\"hardware_profile\",\"duration_ms\",\"samples\",\"warmups\",\"temp_budget_bytes\",\"started_at_unix_ms\",\"elapsed_ms\",\"sample_count\",\"p50_latency_us\",\"p95_latency_us\",\"error_count\",\"budget_status\",\"diagnostic_only\",\"measurement_mode\",\"latency_source\",\"engine_harness\",\"synthetic_model_version\",\"authoritative\",\"can_select_plan_alone\",\"optimizer_boundary\"]}}",
            json_string("andromeda.cli.benchmark.contract.v1"),
            json_string("conservative"),
            architecture,
            profile.has_simd,
            profile.has_direct_io,
            profile.gpu.available,
            BENCHMARK_EVIDENCE_AUTHORITATIVE,
            BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
            json_string(BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY)
        );
        return;
    }

    println!("Andromeda benchmark contract");
    println!("============================");
    println!("surface: administration diagnostics only");
    println!("diagnostic JSON: --diagnostic-json only; not runtime protocol");
    println!("global max duration ms: {MAX_DURATION_MS}");
    println!("global max samples: {MAX_SAMPLES}");
    println!("global max warmups: {MAX_WARMUPS}");
    println!("global max temp bytes: {MAX_TEMP_BYTES}");
    println!("default duration ms: {DEFAULT_DURATION_MS}");
    println!("default samples: {DEFAULT_SAMPLES}");
    println!("default warmups: {DEFAULT_WARMUPS}");
    println!("default temp budget bytes: {DEFAULT_TEMP_BYTES}");
    println!(
        "hardware profile: conservative ({:?})",
        profile.architecture
    );
    println!("authoritative: {}", BENCHMARK_EVIDENCE_AUTHORITATIVE);
    println!(
        "can select plan alone: {}",
        BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE
    );
    println!(
        "optimizer boundary: {}",
        BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY
    );
    println!("exit code 0: command/validation success");
    println!("exit code 1: validation failure");
    println!(
        "required evidence: workload_id, workload_hypothesis, workload_shape_version, workload_size, primary_metric, baseline_ref, budget_origin, decision_linkage, hardware_profile, duration_ms, samples, warmups, temp_budget_bytes, started_at_unix_ms, elapsed_ms, sample_count, p50_latency_us, p95_latency_us, error_count, budget_status, diagnostic_only, measurement_mode, latency_source, engine_harness, synthetic_model_version, authoritative, can_select_plan_alone, optimizer_boundary"
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
            "{{\"id\":{},\"description\":{},\"hypothesis\":{},\"workload_shape_version\":{},\"workload_size\":{},\"primary_metric\":{},\"baseline_ref\":{},\"budget_origin\":{},\"decision_linkage\":{},\"max_duration_ms\":{},\"max_samples\":{},\"max_temp_bytes\":{},\"budget\":{{\"max_p50_latency_us\":{},\"max_p95_latency_us\":{},\"max_error_rate_ppm\":{}}}}}",
            json_string(workload.id),
            json_string(cli_workload_description(workload.id, workload.description)),
            json_string(workload.hypothesis),
            json_string(workload.workload_shape_version),
            json_string(workload.workload_size),
            json_string(workload.primary_metric),
            json_string(workload.baseline_ref),
            json_string(workload.budget_origin),
            json_string(workload.decision_linkage),
            workload.max_duration_ms,
            workload.max_samples,
            workload.max_temp_bytes,
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
        BTREE_NODE_CODEC_WORKLOAD_ID, run_bounded_benchmark,
    };
    use andromeda_bench_workload::BenchmarkRunRequest;

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
        assert!(json.contains("\"workload_hypothesis\":"));
        assert!(json.contains("\"budget_origin\":\"static-workload-registry-v1\""));
        assert!(json.contains("\"decision_linkage\":\"advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass\""));
        assert!(json.contains("\"temp_budget_bytes\":8388608"));
        assert!(json.contains("\"authoritative\":false"));
        assert!(json.contains("\"can_select_plan_alone\":false"));
        assert!(json.contains("\"optimizer_boundary\":\"advisory-only\""));
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
        assert!(human.contains("hypothesis:"));
        assert!(human.contains("budget origin: static-workload-registry-v1"));
        assert!(human.contains(
            "decision linkage: advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass"
        ));
        assert!(human.contains("temp budget bytes: 8388608"));
        assert!(human.contains(&format!(
            "latency source: {BTREE_NODE_CODEC_HARNESS_SOURCE}"
        )));
        assert!(human.contains(&format!("engine harness: {BTREE_NODE_CODEC_HARNESS_NAME}")));
        assert!(human.contains("synthetic model version: none"));
        assert!(human.contains("authoritative: false"));
        assert!(human.contains("can select plan alone: false"));
        assert!(human.contains("optimizer boundary: advisory-only"));
    }
}
