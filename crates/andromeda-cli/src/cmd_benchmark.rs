//! Benchmark administration command scaffold.
//!
//! This command is an operations/diagnostic surface for bounded benchmark
//! orchestration. It does not introduce an application runtime path, SQL, gRPC,
//! or JSON wire semantics. JSON output, when requested, is diagnostic only.

use crate::diagnostic_json::{
    DIAGNOSTIC_JSON_FLAG, JSON_FLAG, json_string, parse_diagnostic_json_flag,
};
use crate::error::cli_error;
use andromeda_bench::{
    BenchmarkError, BenchmarkEvidence, BenchmarkHardwareProfile, BenchmarkRunRequest, BudgetStatus,
    DEFAULT_DURATION_MS, DEFAULT_SAMPLES, DEFAULT_WARMUPS, MAX_DURATION_MS, MAX_SAMPLES,
    MAX_WARMUPS, WORKLOADS, run_bounded_benchmark,
};
use andromeda_core::AndromedaResult;

#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkRunOptions {
    request: BenchmarkRunRequest,
    diagnostic_json: bool,
}

/// Parses and executes benchmark administration subcommands.
pub fn run_benchmark_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("workloads") => {
            let diagnostic_json = has_diagnostic_json_option(&args[1..])?;
            print_workloads(diagnostic_json);
            Ok(())
        }
        Some("contract") => {
            let diagnostic_json = has_diagnostic_json_option(&args[1..])?;
            print_benchmark_contract(diagnostic_json);
            Ok(())
        }
        Some("run") => {
            let options = parse_benchmark_run_options(&args[1..])?;
            let evidence = run_bounded_benchmark(&options.request)
                .map_err(|error| benchmark_error_to_cli_error(error, &options.request))?;
            print_benchmark_run_evidence(&evidence, options.diagnostic_json);
            Ok(())
        }
        Some("-h" | "--help" | "help") | None => {
            print_benchmark_help();
            Ok(())
        }
        Some(cmd) => Err(cli_error(format!(
            "unknown benchmark subcommand `{cmd}`; run `andromeda-cli benchmark --help`"
        ))),
    }
}

fn parse_benchmark_run_options(args: &[String]) -> AndromedaResult<BenchmarkRunOptions> {
    let mut workload_id: Option<String> = None;
    let mut duration_ms = DEFAULT_DURATION_MS;
    let mut samples = DEFAULT_SAMPLES;
    let mut warmups = DEFAULT_WARMUPS;
    let mut hardware_profile = BenchmarkHardwareProfile::Conservative;
    let mut diagnostic_json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--duration-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(cli_error("--duration-ms requires an unsigned integer"));
                };
                duration_ms = parse_u64(value, "--duration-ms")?;
            }
            "--samples" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(cli_error("--samples requires an unsigned integer"));
                };
                samples = parse_u32(value, "--samples")?;
            }
            "--warmups" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(cli_error("--warmups requires an unsigned integer"));
                };
                warmups = parse_u32(value, "--warmups")?;
            }
            "--hardware-profile" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(cli_error("--hardware-profile requires a profile name"));
                };
                hardware_profile = parse_hardware_profile(value)?;
            }
            DIAGNOSTIC_JSON_FLAG => diagnostic_json = true,
            JSON_FLAG => {
                return Err(cli_error(
                    "benchmark uses --diagnostic-json to make JSON diagnostic-only explicit",
                ));
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!("unknown benchmark run option: {opt}")));
            }
            value => {
                if workload_id.is_some() {
                    return Err(cli_error(
                        "benchmark run accepts exactly one workload identifier",
                    ));
                }
                workload_id = Some(value.to_string());
            }
        }

        index += 1;
    }

    let Some(workload_id) = workload_id else {
        return Err(cli_error(
            "usage: andromeda-cli benchmark run <workload> [--duration-ms <ms>] [--samples <n>] [--warmups <n>] [--hardware-profile conservative|declared-local] [--diagnostic-json]",
        ));
    };

    let mut request = BenchmarkRunRequest::new(workload_id);
    request.duration_ms = duration_ms;
    request.samples = samples;
    request.warmups = warmups;
    request.hardware_profile = hardware_profile;

    Ok(BenchmarkRunOptions {
        request,
        diagnostic_json,
    })
}

#[cfg(test)]
fn validate_benchmark_run_options(options: &BenchmarkRunOptions) -> AndromedaResult<()> {
    options
        .request
        .validate()
        .map(|_| ())
        .map_err(|error| benchmark_error_to_cli_error(error, &options.request))
}

fn benchmark_error_to_cli_error(
    error: BenchmarkError,
    request: &BenchmarkRunRequest,
) -> andromeda_core::AndromedaError {
    let message = match error {
        BenchmarkError::EmptyWorkloadId => {
            "benchmark workload identifier must not be empty".to_string()
        }
        BenchmarkError::UnknownWorkload => format!(
            "unknown benchmark workload `{}`; run `andromeda-cli benchmark workloads`",
            request.workload_id
        ),
        BenchmarkError::ZeroDuration => "--duration-ms must be greater than zero".to_string(),
        BenchmarkError::ZeroSamples => "--samples must be greater than zero".to_string(),
        BenchmarkError::DurationExceedsGlobalLimit => {
            format!("--duration-ms must be <= {MAX_DURATION_MS}")
        }
        BenchmarkError::SamplesExceedsGlobalLimit => {
            format!("--samples must be <= {MAX_SAMPLES}")
        }
        BenchmarkError::WarmupsExceedsGlobalLimit => {
            format!("--warmups must be <= {MAX_WARMUPS}")
        }
        BenchmarkError::DurationExceedsWorkloadLimit => {
            match andromeda_bench::find_workload(&request.workload_id) {
                Some(workload) => format!(
                    "workload `{}` duration must be <= {} ms",
                    workload.id, workload.max_duration_ms
                ),
                None => format!(
                    "workload `{}` duration exceeds its bounded limit",
                    request.workload_id
                ),
            }
        }
        BenchmarkError::SamplesExceedsWorkloadLimit => {
            match andromeda_bench::find_workload(&request.workload_id) {
                Some(workload) => format!(
                    "workload `{}` samples must be <= {}",
                    workload.id, workload.max_samples
                ),
                None => format!(
                    "workload `{}` samples exceed its bounded limit",
                    request.workload_id
                ),
            }
        }
        BenchmarkError::InsufficientSamplesForStatistics => {
            "benchmark runner produced no samples".to_string()
        }
    };
    cli_error(message)
}

fn print_benchmark_run_evidence(evidence: &BenchmarkEvidence, diagnostic_json: bool) {
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

fn has_diagnostic_json_option(args: &[String]) -> AndromedaResult<bool> {
    parse_diagnostic_json_flag(args, "benchmark")
}

fn parse_u64(value: &str, option: &str) -> AndromedaResult<u64> {
    value
        .parse::<u64>()
        .map_err(|_| cli_error(format!("{option} expects an unsigned integer")))
}

fn parse_u32(value: &str, option: &str) -> AndromedaResult<u32> {
    value
        .parse::<u32>()
        .map_err(|_| cli_error(format!("{option} expects an unsigned integer")))
}

fn parse_hardware_profile(value: &str) -> AndromedaResult<BenchmarkHardwareProfile> {
    match value {
        "conservative" => Ok(BenchmarkHardwareProfile::Conservative),
        "declared-local" => Ok(BenchmarkHardwareProfile::DeclaredLocal),
        other => Err(cli_error(format!(
            "unknown benchmark hardware profile `{other}`; expected conservative or declared-local"
        ))),
    }
}

fn print_benchmark_help() {
    println!("Andromeda benchmark administration commands");
    println!();
    println!("USAGE: andromeda-cli benchmark <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  workloads           List allowed bounded benchmark workloads");
    println!("  contract            Display benchmark evidence contract and limits");
    println!("  run <workload>      Run a deterministic bounded diagnostic smoke workload");
    println!();
    println!("RUN OPTIONS:");
    println!("  --duration-ms <ms>  Duration cap; global max {MAX_DURATION_MS} ms");
    println!("  --samples <n>       Sample cap; global max {MAX_SAMPLES}");
    println!("  --warmups <n>       Warmup cap; global max {MAX_WARMUPS}");
    println!("  --hardware-profile <conservative|declared-local>");
    println!("  --diagnostic-json   Emit diagnostic machine-readable JSON output");
    println!("  -h, --help          Show this help message");
}

fn print_workloads(diagnostic_json: bool) {
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

fn print_benchmark_contract(diagnostic_json: bool) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn parses_default_run_options() {
        let options = parse_benchmark_run_options(&strings(&["vertical-v0-smoke"])).unwrap();

        assert_eq!(options.request.workload_id, "vertical-v0-smoke");
        assert_eq!(options.request.duration_ms, DEFAULT_DURATION_MS);
        assert_eq!(options.request.samples, DEFAULT_SAMPLES);
        assert_eq!(options.request.warmups, DEFAULT_WARMUPS);
        assert_eq!(
            options.request.hardware_profile,
            BenchmarkHardwareProfile::Conservative
        );
        assert!(!options.diagnostic_json);
    }

    #[test]
    fn validates_bounded_allowed_workload() {
        let options = parse_benchmark_run_options(&strings(&[
            "protocol-smoke-contract",
            "--duration-ms",
            "1000",
            "--samples",
            "5",
            "--warmups",
            "1",
            "--hardware-profile",
            "declared-local",
            "--diagnostic-json",
        ]))
        .unwrap();

        validate_benchmark_run_options(&options).unwrap();
        assert_eq!(
            options.request.hardware_profile,
            BenchmarkHardwareProfile::DeclaredLocal
        );
        assert!(options.diagnostic_json);
    }

    #[test]
    fn rejects_unbounded_duration() {
        let options =
            parse_benchmark_run_options(&strings(&["vertical-v0-smoke", "--duration-ms", "60001"]))
                .unwrap();

        assert!(validate_benchmark_run_options(&options).is_err());
    }

    #[test]
    fn rejects_unknown_workload() {
        let options = parse_benchmark_run_options(&strings(&["unknown-workload"])).unwrap();

        assert!(validate_benchmark_run_options(&options).is_err());
    }

    #[test]
    fn rejects_plain_json_alias() {
        assert!(parse_benchmark_run_options(&strings(&["vertical-v0-smoke", "--json",])).is_err());
        assert!(has_diagnostic_json_option(&strings(&["--json"])).is_err());
    }

    #[test]
    fn run_executes_bench_runner_after_validation() {
        let result = run_benchmark_command(&strings(&[
            "run",
            "vertical-v0-smoke",
            "--duration-ms",
            "1000",
            "--samples",
            "1",
        ]));

        assert!(result.is_ok());
    }

    #[test]
    fn workloads_and_contract_commands_succeed() {
        run_benchmark_command(&strings(&["workloads"])).unwrap();
        run_benchmark_command(&strings(&["contract", "--diagnostic-json"])).unwrap();
    }

    #[test]
    fn hardware_profile_names_are_stable() {
        assert_eq!(
            BenchmarkHardwareProfile::Conservative.as_str(),
            "conservative"
        );
        assert_eq!(
            BenchmarkHardwareProfile::DeclaredLocal.as_str(),
            "declared-local"
        );
    }
}
