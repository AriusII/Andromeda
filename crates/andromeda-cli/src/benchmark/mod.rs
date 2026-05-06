//! Benchmark administration commands.
//!
//! This command is an operations/diagnostic surface for bounded benchmark
//! orchestration. It does not introduce an application runtime path, SQL, gRPC,
//! or JSON wire semantics. JSON output, when requested, is diagnostic only.

mod crud;
mod error;
mod output;
mod parsing;

use crate::error::cli_error;
use andromeda_bench::run_bounded_benchmark;
use andromeda_core::AndromedaResult;

pub(crate) fn run_benchmark_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("workloads") => {
            let diagnostic_json = parsing::has_diagnostic_json_option(&args[1..])?;
            output::print_workloads(diagnostic_json);
            Ok(())
        }
        Some("contract") => {
            let diagnostic_json = parsing::has_diagnostic_json_option(&args[1..])?;
            output::print_benchmark_contract(diagnostic_json);
            Ok(())
        }
        Some("run") => {
            let options = parsing::parse_benchmark_run_options(&args[1..])?;
            let evidence = run_bounded_benchmark(&options.request)
                .map_err(|err| error::benchmark_error_to_cli_error(err, &options.request))?;
            output::print_benchmark_run_evidence(&evidence, options.diagnostic_json);
            Ok(())
        }
        Some("crud-scenarios") => {
            let diagnostic_json = parsing::has_diagnostic_json_option(&args[1..])?;
            crud::print_crud_scenarios(diagnostic_json);
            Ok(())
        }
        Some("crud") => {
            let options = crud::parse_crud_run_options(&args[1..])?;
            let result = crud::run_crud_workload_deterministic(&options.scenario_id, options.seed)?;
            crud::print_crud_result(&result, options.diagnostic_json);
            Ok(())
        }
        Some("-h" | "--help" | "help") | None => {
            output::print_benchmark_help();
            Ok(())
        }
        Some(cmd) => Err(cli_error(format!(
            "unknown benchmark subcommand `{cmd}`; run `andromeda-cli benchmark --help`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
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
    fn crud_scenarios_command_succeeds() {
        run_benchmark_command(&strings(&["crud-scenarios"])).unwrap();
        run_benchmark_command(&strings(&["crud-scenarios", "--diagnostic-json"])).unwrap();
    }

    #[test]
    fn crud_run_command_succeeds() {
        let result = run_benchmark_command(&strings(&[
            "crud",
            "crud-single-1",
            "--seed",
            "42",
            "--diagnostic-json",
        ]));

        assert!(result.is_ok());
    }
}
