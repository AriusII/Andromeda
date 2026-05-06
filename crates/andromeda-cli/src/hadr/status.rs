use andromeda_core::AndromedaResult;

use super::output::print_hadr_status;
use super::parsing::parse_status_json_option;
use super::types::{HadrStatusReport, ReplicaStatus};

pub(super) fn run_hadr_status(args: &[String]) -> AndromedaResult<()> {
    let json_output = parse_status_json_option(args)?;

    let report = HadrStatusReport {
        cluster_role: "Primary".to_string(),
        epoch: 42,
        current_primary: Some(1),
        replicas: vec![
            ReplicaStatus {
                replica_id: 2,
                health_state: "Alive".to_string(),
                received_lsn: 1048576,
                shipped_lsn: 1048576,
                lag_bytes: 0,
            },
            ReplicaStatus {
                replica_id: 3,
                health_state: "Alive".to_string(),
                received_lsn: 1047552,
                shipped_lsn: 1048576,
                lag_bytes: 1024,
            },
        ],
        durable_lsn: 1048576,
        committed_lsn: 1048576,
    };

    print_hadr_status(&report, json_output);
    Ok(())
}
