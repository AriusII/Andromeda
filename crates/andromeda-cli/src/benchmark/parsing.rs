use crate::args::parse_u64_option;
use crate::diagnostic_json::{DIAGNOSTIC_JSON_FLAG, JSON_FLAG, parse_diagnostic_json_flag};
use crate::error::cli_error;
use andromeda_bench::{
    BenchmarkHardwareProfile, BenchmarkRunRequest, DEFAULT_DURATION_MS, DEFAULT_SAMPLES,
    DEFAULT_WARMUPS,
};
use andromeda_core::AndromedaResult;

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
                duration_ms = parse_u64_option(value, "--duration-ms")?;
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

pub(super) fn has_diagnostic_json_option(args: &[String]) -> AndromedaResult<bool> {
    parse_diagnostic_json_flag(args, "benchmark")
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
    use andromeda_bench::{BenchmarkHardwareProfile, MAX_DURATION_MS};

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
        let too_long = (MAX_DURATION_MS + 1).to_string();
        let options = parse_benchmark_run_options(&strings(&[
            "vertical-v0-smoke",
            "--duration-ms",
            too_long.as_str(),
        ]))
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
        assert!(parse_benchmark_run_options(&strings(&["vertical-v0-smoke", "--json"])).is_err());
        assert!(has_diagnostic_json_option(&strings(&["--json"])).is_err());
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
