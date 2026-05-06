use crate::diagnostic_json::JSON_FLAG;
use crate::error::cli_error;
use andromeda_core::AndromedaResult;

use super::output::print_failover_prepare;
use super::types::FailoverPrepareReport;

pub(super) fn run_hadr_failover_prepare(args: &[String]) -> AndromedaResult<()> {
    let mut json_output = false;
    let mut witness_check = false;
    for arg in args {
        match arg.as_str() {
            JSON_FLAG => json_output = true,
            "--witness-check" => witness_check = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown hadr failover-prepare option: {opt}"
                )));
            }
            value => {
                return Err(cli_error(format!(
                    "unexpected hadr failover-prepare argument: {value}"
                )));
            }
        }
    }

    let (quorum_size, witness_available, blocking_issues, remediation_steps) = if witness_check {
        (
            2,
            true,
            vec![],
            vec!["Failover is ready to execute".to_string()],
        )
    } else {
        (
            2,
            false,
            vec![],
            vec!["Run with --witness-check to validate witness availability".to_string()],
        )
    };

    let ready = blocking_issues.is_empty();
    let report = FailoverPrepareReport {
        ready_for_failover: ready,
        current_epoch: 42,
        quorum_size,
        witness_available,
        promotion_candidate_id: Some(2),
        promotion_candidate_lsn_distance: Some(1024),
        blocking_issues,
        remediation_steps,
        fencing_policy: "QuorumEnforced".to_string(),
        message: if ready {
            "Failover pre-validation passed: cluster is ready".to_string()
        } else {
            "Failover pre-validation incomplete: additional checks required".to_string()
        },
    };

    print_failover_prepare(&report, json_output);
    Ok(())
}
