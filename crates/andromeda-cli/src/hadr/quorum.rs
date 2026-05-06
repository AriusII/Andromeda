use andromeda_core::AndromedaResult;
use andromeda_storage::HadrMembershipSnapshot;

use super::output::print_quorum_status;
use super::parsing::parse_quorum_options;
use super::runtime::{load_membership_snapshot, quorum_size};
use super::types::QuorumStatusReport;

pub(super) fn run_hadr_quorum(args: &[String]) -> AndromedaResult<()> {
    let options = parse_quorum_options(args)?;
    let report = match options.membership_store.as_deref() {
        Some(path) => {
            let snapshot = load_membership_snapshot(path)?;
            durable_quorum_report(snapshot.as_ref())
        }
        None => contract_preview_quorum_report(),
    };

    let json_output = options.json_output;
    print_quorum_status(&report, json_output);
    Ok(())
}

fn contract_preview_quorum_report() -> QuorumStatusReport {
    QuorumStatusReport {
        contract_preview: true,
        durable_backend: false,
        total_members: 3,
        quorum_size: 2,
        member_ids: vec![1, 2, 3],
        fencing_policy: "QuorumEnforced".to_string(),
        fencing_status: "not_evaluated_contract_preview".to_string(),
        message:
            "contract preview: durable HADR quorum backend is not wired; showing static command contract only"
                .to_string(),
    }
}

fn durable_quorum_report(snapshot: Option<&HadrMembershipSnapshot>) -> QuorumStatusReport {
    let member_ids = snapshot
        .into_iter()
        .flat_map(HadrMembershipSnapshot::nodes)
        .map(|node| node.id.get())
        .collect::<Vec<_>>();
    let total_members = member_ids.len();
    let has_primary = snapshot.is_some_and(|snapshot| snapshot.primary().is_some());
    let fencing_status = if total_members == 0 {
        "NoMembers"
    } else if has_primary {
        "Active"
    } else {
        "NoPrimary"
    };

    QuorumStatusReport {
        contract_preview: false,
        durable_backend: true,
        total_members,
        quorum_size: quorum_size(total_members),
        member_ids,
        fencing_policy: "QuorumEnforced".to_string(),
        fencing_status: fencing_status.to_string(),
        message:
            "durable HADR membership store loaded; fencing tokens are not attached to CLI runtime yet"
                .to_string(),
    }
}
