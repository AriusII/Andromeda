use andromeda_core::AndromedaResult;
use andromeda_hadr::{HadrMembershipSnapshot, HadrNodeRole};

use super::output::print_hadr_status;
use super::parsing::parse_status_options;
use super::runtime::{load_membership_snapshot, membership_epoch, role_label};
use super::types::{HadrStatusReport, ReplicaStatus};

pub(super) fn run_hadr_status(args: &[String]) -> AndromedaResult<()> {
    let options = parse_status_options(args)?;
    let report = match options.membership_store.as_deref() {
        Some(path) => {
            let snapshot = load_membership_snapshot(path)?;
            durable_status_report(snapshot.as_ref())
        },
        None => contract_preview_status_report(),
    };

    let json_output = options.json_output;
    print_hadr_status(&report, json_output);
    Ok(())
}

fn contract_preview_status_report() -> HadrStatusReport {
    HadrStatusReport {
        contract_preview: true,
        durable_backend: false,
        cluster_role: "contract_scaffold_primary".to_string(),
        epoch: 42,
        current_primary: Some(1),
        replicas: vec![
            ReplicaStatus {
                replica_id: 2,
                health_state: "contract_scaffold_alive".to_string(),
                received_lsn: 0,
                shipped_lsn: 0,
                lag_bytes: 0,
            },
            ReplicaStatus {
                replica_id: 3,
                health_state: "contract_scaffold_alive".to_string(),
                received_lsn: 0,
                shipped_lsn: 0,
                lag_bytes: 0,
            },
        ],
        durable_lsn: 0,
        committed_lsn: 0,
        message:
            "contract preview: durable HADR status backend is not wired; showing static command contract only"
                .to_string(),
    }
}

fn durable_status_report(snapshot: Option<&HadrMembershipSnapshot>) -> HadrStatusReport {
    let current_primary =
        snapshot.and_then(|snapshot| snapshot.primary().map(|node| node.id.get()));
    let replicas = snapshot
        .into_iter()
        .flat_map(HadrMembershipSnapshot::nodes)
        .filter(|node| node.role != HadrNodeRole::Primary)
        .map(|node| ReplicaStatus {
            replica_id: node.id.get(),
            health_state: role_label(node.role).to_string(),
            received_lsn: 0,
            shipped_lsn: 0,
            lag_bytes: 0,
        })
        .collect::<Vec<_>>();

    let cluster_role = if snapshot.is_none_or(|snapshot| snapshot.nodes().is_empty()) {
        "Unconfigured"
    } else if current_primary.is_some() {
        "PrimaryPresent"
    } else {
        "NoPrimary"
    };

    HadrStatusReport {
        contract_preview: false,
        durable_backend: true,
        cluster_role: cluster_role.to_string(),
        epoch: membership_epoch(snapshot),
        current_primary,
        replicas,
        durable_lsn: 0,
        committed_lsn: 0,
        message:
            "durable HADR membership store loaded; WAL shipping LSN telemetry is not attached yet"
                .to_string(),
    }
}
