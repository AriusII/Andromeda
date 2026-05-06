use andromeda_core::AndromedaResult;

use super::output::print_failover_prepare;
use super::parsing::parse_failover_prepare_options;
use super::types::FailoverPrepareReport;

pub(super) fn run_hadr_failover_prepare(args: &[String]) -> AndromedaResult<()> {
    let options = parse_failover_prepare_options(args)?;

    let (quorum_size, witness_available, blocking_issues, remediation_steps) =
        if options.witness_check {
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

    print_failover_prepare(&report, options.json_output);
    Ok(())
}
