use andromeda_core::AndromedaResult;

use super::output::print_quorum_status;
use super::parsing::parse_quorum_json_option;
use super::types::QuorumStatusReport;

pub(super) fn run_hadr_quorum(args: &[String]) -> AndromedaResult<()> {
    let json_output = parse_quorum_json_option(args)?;

    let report = QuorumStatusReport {
        total_members: 3,
        quorum_size: 2,
        member_ids: vec![1, 2, 3],
        fencing_policy: "QuorumEnforced".to_string(),
        fencing_status: "Active".to_string(),
    };

    print_quorum_status(&report, json_output);
    Ok(())
}
