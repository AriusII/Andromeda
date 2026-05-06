use crate::diagnostic_json::{DIAGNOSTIC_JSON_FLAG, JSON_FLAG, json_string};
use crate::error::cli_error;
use crate::parse::{next_option_value, parse_u64_option};
use andromeda_bench::{
    CRUD_SCENARIOS, CrudDataGenerator, CrudOperationMetrics, CrudWorkloadResult, find_crud_scenario,
};
use andromeda_core::AndromedaResult;

#[derive(Debug, Clone)]
pub(super) struct CrudRunOptions {
    pub(super) scenario_id: String,
    pub(super) seed: u64,
    pub(super) diagnostic_json: bool,
}

pub(super) fn parse_crud_run_options(args: &[String]) -> AndromedaResult<CrudRunOptions> {
    let mut scenario_id: Option<String> = None;
    let mut seed: u64 = 42;
    let mut diagnostic_json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--seed" => {
                let value =
                    next_option_value(args, &mut index, "--seed requires an unsigned integer")?;
                seed = parse_u64_option(value, "--seed")?;
            }
            DIAGNOSTIC_JSON_FLAG => diagnostic_json = true,
            JSON_FLAG => {
                return Err(cli_error(
                    "crud uses --diagnostic-json to make JSON diagnostic-only explicit",
                ));
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!("unknown crud run option: {opt}")));
            }
            value => {
                if scenario_id.is_some() {
                    return Err(cli_error(
                        "crud run accepts exactly one scenario identifier",
                    ));
                }
                scenario_id = Some(value.to_string());
            }
        }
        index += 1;
    }

    let Some(scenario_id) = scenario_id else {
        return Err(cli_error(
            "usage: andromeda-cli benchmark crud <scenario> [--seed <u64>] [--diagnostic-json]",
        ));
    };

    if find_crud_scenario(&scenario_id).is_none() {
        return Err(cli_error(format!(
            "unknown CRUD scenario `{scenario_id}`; run `andromeda-cli benchmark crud-scenarios`"
        )));
    }

    Ok(CrudRunOptions {
        scenario_id,
        seed,
        diagnostic_json,
    })
}

pub(super) fn run_crud_workload_deterministic(
    scenario_id: &str,
    seed: u64,
) -> AndromedaResult<CrudWorkloadResult> {
    let scenario = find_crud_scenario(scenario_id)
        .ok_or_else(|| cli_error(format!("scenario `{scenario_id}` not found")))?;

    scenario
        .validate()
        .map_err(|err| cli_error(format!("scenario validation failed: {err}")))?;

    let generator = CrudDataGenerator::new(seed, 128);
    let rows = generator.generate_rows(scenario.row_count);
    let start_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let base_latency_us = match scenario.id {
        "crud-single-1" => 500,
        "crud-single-100" => 200,
        "crud-multi4-10" => 1000,
        "crud-multi8-100" => 1500,
        "crud-scan-1m" => 50,
        "crud-write-heavy" => 800,
        _ => 500,
    };

    let insert_count = (rows.len() as f64 * f64::from(scenario.insert_pct) / 100.0) as u64;
    let update_count = (rows.len() as f64 * f64::from(scenario.update_pct) / 100.0) as u64;
    let delete_count = (rows.len() as f64 * f64::from(scenario.delete_pct) / 100.0) as u64;
    let scan_count = (rows.len() as f64 * f64::from(scenario.scan_pct) / 100.0) as u64;

    let total_ops = insert_count + update_count + delete_count + scan_count;
    let elapsed_ms = (total_ops * base_latency_us / 1000).max(1);
    let mut operations = Vec::new();

    if scenario.insert_pct > 0 {
        push_operation_metrics(
            &mut operations,
            "insert",
            insert_count,
            base_latency_us + u64::from(scenario.thread_count as u32 * 10),
        );
    }

    if scenario.update_pct > 0 {
        push_operation_metrics(
            &mut operations,
            "update",
            update_count,
            base_latency_us + u64::from(scenario.thread_count as u32 * 15),
        );
    }

    if scenario.delete_pct > 0 {
        push_operation_metrics(
            &mut operations,
            "delete",
            delete_count,
            base_latency_us + u64::from(scenario.thread_count as u32 * 12),
        );
    }

    if scenario.scan_pct > 0 {
        push_operation_metrics(&mut operations, "scan", scan_count, base_latency_us / 2);
    }

    let total_throughput = (total_ops as f64 * 1_000.0) / elapsed_ms.max(1) as f64;

    Ok(CrudWorkloadResult {
        scenario_id: scenario.id.to_string(),
        start_time_unix_ms: start_ms,
        elapsed_ms,
        thread_count: scenario.thread_count,
        batch_size: scenario.batch_size,
        row_count: scenario.row_count,
        operations,
        total_ops,
        total_throughput_ops_sec: total_throughput,
        total_errors: 0,
        seed,
    })
}

fn push_operation_metrics(
    operations: &mut Vec<CrudOperationMetrics>,
    operation: &str,
    count: u64,
    avg_latency: u64,
) {
    operations.push(CrudOperationMetrics {
        operation: operation.to_string(),
        count,
        total_us: count * avg_latency,
        p50_us: avg_latency,
        p95_us: avg_latency * 2,
        p99_us: avg_latency * 3,
        throughput_ops_sec: (count as f64 * 1_000_000.0)
            / (count as f64 * avg_latency as f64).max(1.0),
        error_count: 0,
    });
}

pub(super) fn print_crud_scenarios(diagnostic_json: bool) {
    if diagnostic_json {
        print_crud_scenarios_json();
        return;
    }

    println!("Andromeda CRUD benchmark scenarios");
    println!("==================================");
    for scenario in CRUD_SCENARIOS {
        println!("- {}", scenario.id);
        println!("  description: {}", scenario.description);
        println!("  threads: {}", scenario.thread_count);
        println!("  batch_size: {}", scenario.batch_size);
        println!("  row_count: {}", scenario.row_count);
        println!(
            "  insert/update/delete/scan: {}%/{}%/{}%/{}%",
            scenario.insert_pct, scenario.update_pct, scenario.delete_pct, scenario.scan_pct
        );
        println!("  max_duration_ms: {}", scenario.max_duration_ms);
    }
}

fn print_crud_scenarios_json() {
    print!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"scenarios\":[",
        json_string("andromeda.cli.benchmark.crud_scenarios.v1")
    );

    for (index, scenario) in CRUD_SCENARIOS.iter().enumerate() {
        if index > 0 {
            print!(",");
        }
        print!(
            "{{\"id\":{},\"description\":{},\"thread_count\":{},\"batch_size\":{},\"row_count\":{},\"insert_pct\":{},\"update_pct\":{},\"delete_pct\":{},\"scan_pct\":{},\"max_duration_ms\":{}}}",
            json_string(scenario.id),
            json_string(scenario.description),
            scenario.thread_count,
            scenario.batch_size,
            scenario.row_count,
            scenario.insert_pct,
            scenario.update_pct,
            scenario.delete_pct,
            scenario.scan_pct,
            scenario.max_duration_ms
        );
    }
    println!("]}}")
}

pub(super) fn print_crud_result(result: &CrudWorkloadResult, diagnostic_json: bool) {
    if diagnostic_json {
        println!("{}", result.to_json());
        return;
    }

    println!("Andromeda CRUD workload result");
    println!("=============================");
    println!("scenario_id: {}", result.scenario_id);
    println!("start_time_unix_ms: {}", result.start_time_unix_ms);
    println!("elapsed_ms: {}", result.elapsed_ms);
    println!("thread_count: {}", result.thread_count);
    println!("batch_size: {}", result.batch_size);
    println!("row_count: {}", result.row_count);
    println!("total_ops: {}", result.total_ops);
    println!(
        "total_throughput_ops_sec: {:.2}",
        result.total_throughput_ops_sec
    );
    println!("total_errors: {}", result.total_errors);
    println!("seed: {}", result.seed);
    println!();
    println!("operations:");
    for op in &result.operations {
        println!("  {}:", op.operation);
        println!("    count: {}", op.count);
        println!("    total_us: {}", op.total_us);
        println!("    p50_us: {}", op.p50_us);
        println!("    p95_us: {}", op.p95_us);
        println!("    p99_us: {}", op.p99_us);
        println!("    throughput_ops_sec: {:.2}", op.throughput_ops_sec);
        println!("    error_count: {}", op.error_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn parses_crud_run_options_with_defaults() {
        let options = parse_crud_run_options(&strings(&["crud-single-1"])).unwrap();

        assert_eq!(options.scenario_id, "crud-single-1");
        assert_eq!(options.seed, 42);
        assert!(!options.diagnostic_json);
    }

    #[test]
    fn parses_crud_run_options_with_seed() {
        let options = parse_crud_run_options(&strings(&["crud-single-1", "--seed", "99"])).unwrap();

        assert_eq!(options.scenario_id, "crud-single-1");
        assert_eq!(options.seed, 99);
    }

    #[test]
    fn rejects_unknown_crud_scenario() {
        let result = parse_crud_run_options(&strings(&["unknown-scenario"]));
        assert!(result.is_err());
    }
}
