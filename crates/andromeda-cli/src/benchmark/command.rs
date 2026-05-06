use crate::error::cli_error;
use andromeda_bench::run_bounded_benchmark;
use andromeda_core::AndromedaResult;

pub fn run_benchmark_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("workloads") => {
            let diagnostic_json = super::parse::has_diagnostic_json_option(&args[1..])?;
            super::output::print_workloads(diagnostic_json);
            Ok(())
        }
        Some("contract") => {
            let diagnostic_json = super::parse::has_diagnostic_json_option(&args[1..])?;
            super::output::print_benchmark_contract(diagnostic_json);
            Ok(())
        }
        Some("run") => {
            let options = super::parse::parse_benchmark_run_options(&args[1..])?;
            let evidence = run_bounded_benchmark(&options.request)
                .map_err(|err| super::error::benchmark_error_to_cli_error(err, &options.request))?;
            super::output::print_benchmark_run_evidence(&evidence, options.diagnostic_json);
            Ok(())
        }
        Some("crud-scenarios") => {
            let diagnostic_json = super::parse::has_diagnostic_json_option(&args[1..])?;
            super::crud::print_crud_scenarios(diagnostic_json);
            Ok(())
        }
        Some("crud") => {
            let options = super::crud::parse_crud_run_options(&args[1..])?;
            let result =
                super::crud::run_crud_workload_deterministic(&options.scenario_id, options.seed)?;
            super::crud::print_crud_result(&result, options.diagnostic_json);
            Ok(())
        }
        Some("-h" | "--help" | "help") | None => {
            super::output::print_benchmark_help();
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
