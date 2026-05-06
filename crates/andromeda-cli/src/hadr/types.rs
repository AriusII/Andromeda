#[derive(Debug, Clone)]
pub(crate) struct HadrStatusReport {
    pub(crate) cluster_role: String,
    pub(crate) epoch: u64,
    pub(crate) current_primary: Option<u64>,
    pub(crate) replicas: Vec<ReplicaStatus>,
    pub(crate) durable_lsn: u64,
    pub(crate) committed_lsn: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ReplicaStatus {
    pub(crate) replica_id: u64,
    pub(crate) health_state: String,
    pub(crate) received_lsn: u64,
    pub(crate) shipped_lsn: u64,
    pub(crate) lag_bytes: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct QuorumStatusReport {
    pub(crate) total_members: usize,
    pub(crate) quorum_size: usize,
    pub(crate) member_ids: Vec<u64>,
    pub(crate) fencing_policy: String,
    pub(crate) fencing_status: String,
}

#[derive(Debug, Clone)]
pub(crate) struct NodeManagementReport {
    pub(crate) action: String,
    pub(crate) node_id: Option<u64>,
    pub(crate) role: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) would_apply: bool,
    pub(crate) quorum_check: String,
    pub(crate) fencing_check: String,
    pub(crate) audit_event: String,
    pub(crate) required_permissions: Vec<String>,
    pub(crate) failure_mode: Option<String>,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PromotionOutcome {
    pub(crate) success: bool,
    pub(crate) new_epoch: u64,
    pub(crate) promoted_replica_id: u64,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DemotionOutcome {
    pub(crate) success: bool,
    pub(crate) new_primary_id: Option<u64>,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct FailoverPrepareReport {
    pub(crate) ready_for_failover: bool,
    pub(crate) current_epoch: u64,
    pub(crate) quorum_size: usize,
    pub(crate) witness_available: bool,
    pub(crate) promotion_candidate_id: Option<u64>,
    pub(crate) promotion_candidate_lsn_distance: Option<i64>,
    pub(crate) blocking_issues: Vec<String>,
    pub(crate) remediation_steps: Vec<String>,
    pub(crate) fencing_policy: String,
    pub(crate) message: String,
}
