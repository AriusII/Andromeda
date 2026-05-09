use crate::diagnostic_json::{DIAGNOSTIC_JSON_FLAG, JSON_FLAG};
use crate::error::cli_error;
use crate::parse::{next_option_value_rejecting_flag, parse_u32_option, parse_u64_option};
use andromeda_bench::{
    BenchmarkHardwareProfile, BenchmarkRunRequest, DEFAULT_DURATION_MS, DEFAULT_SAMPLES,
    DEFAULT_TEMP_BYTES, DEFAULT_WARMUPS,
};
use andromeda_error::AndromedaResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BenchmarkRunOptions {
    pub(super) request: BenchmarkRunRequest,
    pub(super) diagnostic_json: bool,
}

pub(super) fn parse_benchmark_run_options(args: &[String]) -> AndromedaResult<BenchmarkRunOptions> {
    let mut workload_id: Option<String> = None;
    let mut duration_ms = DEFAULT_DURATION_MS;
    let mut samples = DEFAULT_SAMPLES;
    let mut warmups = DEFAULT_WARMUPS;
    let mut temp_budget_bytes = DEFAULT_TEMP_BYTES;
    let mut hardware_profile = BenchmarkHardwareProfile::Conservative;
    let mut diagnostic_json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--duration-ms" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--duration-ms requires an unsigned integer",
                )?;
                duration_ms = parse_u64_option(value, "--duration-ms")?;
            },
            "--samples" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--samples requires an unsigned integer",
                )?;
                samples = parse_u32_option(value, "--samples")?;
            },
            "--warmups" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--warmups requires an unsigned integer",
                )?;
                warmups = parse_u32_option(value, "--warmups")?;
            },
            "--temp-budget-bytes" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--temp-budget-bytes requires an unsigned integer",
                )?;
                temp_budget_bytes = parse_u64_option(value, "--temp-budget-bytes")?;
            },
            "--hardware-profile" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--hardware-profile requires a profile name",
                )?;
                hardware_profile = parse_hardware_profile(value)?;
            },
            DIAGNOSTIC_JSON_FLAG => diagnostic_json = true,
            JSON_FLAG => {
                return Err(cli_error(
                    "benchmark uses --diagnostic-json to make JSON diagnostic-only explicit",
                ));
            },
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown benchmark run option; supported options are --duration-ms, --samples, --warmups, --temp-budget-bytes, --hardware-profile, and --diagnostic-json",
                ));
            },
            value => {
                if workload_id.is_some() {
                    return Err(cli_error(
                        "benchmark run accepts exactly one workload identifier",
                    ));
                }
                workload_id = Some(value.to_string());
            },
        }

        index += 1;
    }

    let Some(workload_id) = workload_id else {
        return Err(cli_error(
            "usage: andromeda-cli benchmark run <workload> [--duration-ms <ms>] [--samples <n>] [--warmups <n>] [--temp-budget-bytes <bytes>] [--hardware-profile conservative|declared-local] [--diagnostic-json]",
        ));
    };

    let mut request = BenchmarkRunRequest::new(workload_id);
    request.duration_ms = duration_ms;
    request.samples = samples;
    request.warmups = warmups;
    request.temp_budget_bytes = temp_budget_bytes;
    request.hardware_profile = hardware_profile;

    Ok(BenchmarkRunOptions {
        request,
        diagnostic_json,
    })
}

pub(super) fn has_diagnostic_json_option(args: &[String]) -> AndromedaResult<bool> {
    let mut diagnostic_json = false;
    for arg in args {
        match arg.as_str() {
            DIAGNOSTIC_JSON_FLAG => diagnostic_json = true,
            JSON_FLAG => {
                return Err(cli_error(
                    "benchmark uses --diagnostic-json to make JSON diagnostic-only explicit",
                ));
            },
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown benchmark option; supported output option is --diagnostic-json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected benchmark argument; supported output option is --diagnostic-json",
                ));
            },
        }
    }
    Ok(diagnostic_json)
}

fn parse_hardware_profile(value: &str) -> AndromedaResult<BenchmarkHardwareProfile> {
    match value {
        "conservative" => Ok(BenchmarkHardwareProfile::Conservative),
        "declared-local" => Ok(BenchmarkHardwareProfile::DeclaredLocal),
        _ => Err(cli_error(
            "unknown benchmark hardware profile; expected conservative or declared-local",
        )),
    }
}

#[cfg(test)]
pub(super) fn validate_benchmark_run_options(options: &BenchmarkRunOptions) -> AndromedaResult<()> {
    use crate::benchmark::error::benchmark_error_to_cli_error;

    options
        .request
        .validate()
        .map(|_| ())
        .map_err(|error| benchmark_error_to_cli_error(error, &options.request))
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_bench::{BenchmarkHardwareProfile, MAX_DURATION_MS, MAX_TEMP_BYTES};

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn parses_default_run_options() {
        let options =
            parse_benchmark_run_options(&strings(&["inventory-recoverable-smoke"])).unwrap();

        assert_eq!(options.request.workload_id, "inventory-recoverable-smoke");
        assert_eq!(options.request.duration_ms, DEFAULT_DURATION_MS);
        assert_eq!(options.request.samples, DEFAULT_SAMPLES);
        assert_eq!(options.request.warmups, DEFAULT_WARMUPS);
        assert_eq!(options.request.temp_budget_bytes, DEFAULT_TEMP_BYTES);
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
            "--temp-budget-bytes",
            "1048576",
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
        assert_eq!(options.request.temp_budget_bytes, 1_048_576);
        assert!(options.diagnostic_json);
    }

    #[test]
    fn rejects_unbounded_duration() {
        let too_long = (MAX_DURATION_MS + 1).to_string();
        let options = parse_benchmark_run_options(&strings(&[
            "inventory-recoverable-smoke",
            "--duration-ms",
            too_long.as_str(),
        ]))
        .unwrap();

        assert!(validate_benchmark_run_options(&options).is_err());
    }

    #[test]
    fn rejects_invalid_temp_budget_with_user_facing_messages() {
        let options = parse_benchmark_run_options(&strings(&[
            "inventory-recoverable-smoke",
            "--temp-budget-bytes",
            "0",
        ]))
        .unwrap();
        let err = validate_benchmark_run_options(&options).unwrap_err();
        assert_eq!(
            err.message(),
            "--temp-budget-bytes must be greater than zero"
        );

        let too_large = (MAX_TEMP_BYTES + 1).to_string();
        let options = parse_benchmark_run_options(&strings(&[
            "inventory-recoverable-smoke",
            "--temp-budget-bytes",
            too_large.as_str(),
        ]))
        .unwrap();
        let err = validate_benchmark_run_options(&options).unwrap_err();
        assert_eq!(
            err.message(),
            format!("--temp-budget-bytes must be <= {MAX_TEMP_BYTES}")
        );
    }

    #[test]
    fn rejects_unknown_workload() {
        let options = parse_benchmark_run_options(&strings(&["unknown-workload"])).unwrap();

        assert!(validate_benchmark_run_options(&options).is_err());
    }

    #[test]
    fn rejects_plain_json_alias() {
        assert!(
            parse_benchmark_run_options(&strings(&["inventory-recoverable-smoke", "--json"]))
                .is_err()
        );
        assert!(has_diagnostic_json_option(&strings(&["--json"])).is_err());
    }

    #[test]
    fn rejects_flag_as_numeric_option_value() {
        let err = parse_benchmark_run_options(&strings(&[
            "inventory-recoverable-smoke",
            "--duration-ms",
            "--samples",
        ]))
        .unwrap_err();

        assert_eq!(err.message(), "--duration-ms requires an unsigned integer");
    }

    #[test]
    fn benchmark_option_errors_do_not_echo_values() {
        let err = parse_benchmark_run_options(&strings(&[
            "inventory-recoverable-smoke",
            "--token=super-secret",
        ]))
        .unwrap_err();
        assert_eq!(
            err.message(),
            "unknown benchmark run option; supported options are --duration-ms, --samples, --warmups, --temp-budget-bytes, --hardware-profile, and --diagnostic-json"
        );
        assert!(!err.message().contains("super-secret"));

        let err = parse_benchmark_run_options(&strings(&[
            "inventory-recoverable-smoke",
            "--hardware-profile",
            "super-secret",
        ]))
        .unwrap_err();
        assert_eq!(
            err.message(),
            "unknown benchmark hardware profile; expected conservative or declared-local"
        );
        assert!(!err.message().contains("super-secret"));
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
