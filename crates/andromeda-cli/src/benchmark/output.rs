use crate::diagnostic_json::json_string;
use andromeda_bench::{
    BenchmarkEvidence, BenchmarkHardwareProfile, BudgetStatus, DEFAULT_DURATION_MS,
    DEFAULT_SAMPLES, DEFAULT_WARMUPS, MAX_DURATION_MS, MAX_SAMPLES, MAX_WARMUPS, WORKLOADS,
};

pub(super) fn print_benchmark_run_evidence(evidence: &BenchmarkEvidence, diagnostic_json: bool) {
    if diagnostic_json {
        print_benchmark_run_json(evidence);
        return;
    }

    println!("Andromeda benchmark diagnostic evidence");
    println!("=======================================");
    println!("runner: deterministic-smoke");
    println!("workload id: {}", evidence.workload_id);
    println!("hardware profile: {}", evidence.hardware_profile.as_str());
    println!("duration ms: {}", evidence.duration_ms);
    println!("samples: {}", evidence.samples);
    println!("warmups: {}", evidence.warmups);
    println!("started at unix ms: {}", evidence.started_at_unix_ms);
    println!("elapsed ms: {}", evidence.elapsed_ms);
    println!("sample count: {}", evidence.sample_count);
    println!("p50 latency us: {}", evidence.p50_latency_us);
    println!("p95 latency us: {}", evidence.p95_latency_us);
    println!("error count: {}", evidence.error_count);
    println!(
        "budget status: {}",
        budget_status_str(evidence.budget_status)
    );
    println!("diagnostic only: {}", evidence.diagnostic_only);
}

fn print_benchmark_run_json(evidence: &BenchmarkEvidence) {
    println!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"runner\":{},\"evidence\":{{\"workload_id\":{},\"hardware_profile\":{},\"duration_ms\":{},\"samples\":{},\"warmups\":{},\"started_at_unix_ms\":{},\"elapsed_ms\":{},\"sample_count\":{},\"p50_latency_us\":{},\"p95_latency_us\":{},\"error_count\":{},\"budget_status\":{},\"diagnostic_only\":{}}}}}",
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
        evidence.diagnostic_only
    );
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
    if diagnostic_json {
        print_workloads_json();
        return;
    }

    println!("Andromeda benchmark workloads");
    println!("=============================");
    for workload in WORKLOADS {
        println!("- {}", workload.id);
        println!("  description: {}", workload.description);
        println!("  max duration ms: {}", workload.max_duration_ms);
        println!("  max samples: {}", workload.max_samples);
        println!("  budget p50 us: {}", workload.budget.max_p50_latency_us);
        println!("  budget p95 us: {}", workload.budget.max_p95_latency_us);
        println!("  budget error ppm: {}", workload.budget.max_error_rate_ppm);
    }
}

pub(super) fn print_benchmark_contract(diagnostic_json: bool) {
    let profile = BenchmarkHardwareProfile::Conservative.materialize();

    if diagnostic_json {
        let architecture = json_string(&format!("{:?}", profile.architecture));
        println!(
            "{{\"schema\":{},\"diagnostic_only\":true,\"global_limits\":{{\"max_duration_ms\":{MAX_DURATION_MS},\"max_samples\":{MAX_SAMPLES},\"max_warmups\":{MAX_WARMUPS}}},\"default_limits\":{{\"duration_ms\":{DEFAULT_DURATION_MS},\"samples\":{DEFAULT_SAMPLES},\"warmups\":{DEFAULT_WARMUPS}}},\"hardware_profile\":{{\"name\":{},\"architecture\":{},\"has_simd\":{},\"has_direct_io\":{},\"gpu_available\":{}}},\"exit_codes\":{{\"success\":0,\"validation_failure\":1}},\"evidence_fields\":[\"workload_id\",\"hardware_profile\",\"duration_ms\",\"samples\",\"warmups\",\"started_at_unix_ms\",\"elapsed_ms\",\"sample_count\",\"p50_latency_us\",\"p95_latency_us\",\"error_count\",\"budget_status\",\"diagnostic_only\"]}}",
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
        "required evidence: workload_id, hardware_profile, duration_ms, samples, warmups, started_at_unix_ms, elapsed_ms, sample_count, p50_latency_us, p95_latency_us, error_count, budget_status, diagnostic_only"
    );
}

fn print_workloads_json() {
    print!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"workloads\":[",
        json_string("andromeda.cli.benchmark.workloads.v1")
    );
    for (index, workload) in WORKLOADS.iter().enumerate() {
        if index > 0 {
            print!(",");
        }
        print!(
            "{{\"id\":{},\"description\":{},\"max_duration_ms\":{},\"max_samples\":{},\"budget\":{{\"max_p50_latency_us\":{},\"max_p95_latency_us\":{},\"max_error_rate_ppm\":{}}}}}",
            json_string(workload.id),
            json_string(workload.description),
            workload.max_duration_ms,
            workload.max_samples,
            workload.budget.max_p50_latency_us,
            workload.budget.max_p95_latency_us,
            workload.budget.max_error_rate_ppm
        );
    }
    println!("]}}");
}
