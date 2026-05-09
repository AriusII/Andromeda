use std::path::Path;

use crate::error::cli_error;
use crate::parse::parse_u64;
use andromeda_core::AndromedaResult;
use andromeda_hadr::{
    HadrClusterOperation, HadrClusterSecurityEvidence, HadrFencingContext, HadrFencingToken,
    HadrMembershipSnapshot, HadrMembershipStore, HadrNodeId, HadrPromotionAuditLog,
    HadrPromotionVote, PromotionAttempt, PromotionBoundary, PromotionCommit, PromotionPlanner,
};
use andromeda_observe::{
    CertificateIdentity, Permission, SecurityAuditOutcome, SecurityAuditTrace,
    SecurityPolicyVersionEvidence, SurfaceScope, TraceId, UserPrincipal, UserPrincipalKind,
};
use andromeda_wal::Lsn;

use super::output::print_promotion_outcome;
use super::parsing::parse_promote_options;
use super::runtime::{
    FileBackedPromotionAuditLog, default_promotion_audit_log_path, open_membership_store,
};
use super::types::PromotionOutcome;

pub(super) fn run_hadr_promote(args: &[String]) -> AndromedaResult<()> {
    if args.is_empty() {
        return Err(cli_error(
            "hadr promote requires <replica-id>; usage: `hadr promote <replica-id> --dry-run`",
        ));
    }

    let replica_id = parse_u64(&args[0], "replica-id must be an unsigned integer")?;
    let options = parse_promote_options(&args[1..])?;

    if let Some(path) = options.membership_store.as_deref() {
        return run_durable_hadr_promote(replica_id, &options, path);
    }
    if options.apply {
        return Err(cli_error(
            "hadr promote --apply requires --membership-store <file>",
        ));
    }
    if !options.dry_run {
        return Err(cli_error(
            "hadr promote is contract preview only until a durable HADR backend is wired; rerun with --dry-run",
        ));
    }

    let outcome = PromotionOutcome {
        success: true,
        dry_run: true,
        would_apply: false,
        contract_preview: true,
        durable_backend: false,
        new_epoch: 43,
        promoted_replica_id: replica_id,
        candidate_lsn: None,
        audit_lsn: None,
        committed_safe_lsn: None,
        quorum_size: None,
        granted_votes: None,
        fencing_primary_id: None,
        fencing_epoch: None,
        audit_log: None,
        message: format!(
            "dry-run accepted: replica {} promotion would require durable epoch, quorum, fencing, and audit updates",
            replica_id
        ),
    };

    print_promotion_outcome(&outcome, options.json_output);
    Ok(())
}

fn run_durable_hadr_promote(
    replica_id: u64,
    options: &super::parsing::PromoteOptions,
    membership_store_path: &Path,
) -> AndromedaResult<()> {
    if !options.apply && !options.dry_run {
        return Err(cli_error(
            "hadr promote with --membership-store requires --apply or --dry-run",
        ));
    }

    let store = open_membership_store(membership_store_path)?;
    let snapshot = store
        .load()?
        .ok_or_else(|| cli_error("hadr promote requires a non-empty membership store"))?;
    let attempt = build_promotion_attempt(replica_id, options, &snapshot)?;

    if options.dry_run {
        let plan = PromotionPlanner::plan(&snapshot, attempt)?;
        let outcome = PromotionOutcome {
            success: true,
            dry_run: true,
            would_apply: true,
            contract_preview: false,
            durable_backend: true,
            new_epoch: plan.proposed_epoch.get(),
            promoted_replica_id: replica_id,
            candidate_lsn: options.candidate_lsn,
            audit_lsn: options.audit_lsn,
            committed_safe_lsn: Some(plan.committed_safe_lsn.get()),
            quorum_size: Some(plan.marker.audit_record.quorum_size),
            granted_votes: Some(plan.marker.audit_record.granted_votes),
            fencing_primary_id: plan
                .marker
                .audit_record
                .active_token
                .map(|token| token.primary_id.get()),
            fencing_epoch: plan
                .marker
                .audit_record
                .active_token
                .map(|token| token.epoch.get()),
            audit_log: None,
            message: format!(
                "dry-run accepted: durable runtime promotion plan validated for replica {} with quorum, LSN, fencing, and audit evidence",
                replica_id
            ),
        };
        print_promotion_outcome(&outcome, options.json_output);
        return Ok(());
    }

    let audit_log_path = options
        .promotion_audit_log
        .clone()
        .unwrap_or_else(|| default_promotion_audit_log_path(membership_store_path));
    let audit_log = FileBackedPromotionAuditLog::open(audit_log_path);
    let commit = apply_durable_promotion_with_audit(&store, &audit_log, attempt)?;
    let outcome = PromotionOutcome {
        success: true,
        dry_run: false,
        would_apply: true,
        contract_preview: false,
        durable_backend: true,
        new_epoch: commit.snapshot.epoch().get(),
        promoted_replica_id: replica_id,
        candidate_lsn: options.candidate_lsn,
        audit_lsn: options.audit_lsn,
        committed_safe_lsn: Some(commit.marker.committed_safe_lsn.get()),
        quorum_size: Some(commit.marker.audit_record.quorum_size),
        granted_votes: Some(commit.marker.audit_record.granted_votes),
        fencing_primary_id: commit
            .marker
            .audit_record
            .active_token
            .map(|token| token.primary_id.get()),
        fencing_epoch: commit
            .marker
            .audit_record
            .active_token
            .map(|token| token.epoch.get()),
        audit_log: Some(audit_log.path().display().to_string()),
        message: format!(
            "durable HADR membership store promoted replica {} to primary after audit marker",
            replica_id
        ),
    };
    print_promotion_outcome(&outcome, options.json_output);
    Ok(())
}

pub(super) fn apply_durable_promotion_with_audit<S, A>(
    store: &S,
    audit_log: &A,
    attempt: PromotionAttempt,
) -> AndromedaResult<PromotionCommit>
where
    S: HadrMembershipStore,
    A: HadrPromotionAuditLog,
{
    PromotionBoundary::new(store, audit_log)
        .promote_with_cluster_security(attempt, cluster_promotion_security_evidence()?)
}

fn cluster_promotion_security_evidence() -> AndromedaResult<HadrClusterSecurityEvidence> {
    HadrClusterSecurityEvidence::new(
        HadrClusterOperation::PromotePrimary,
        SecurityAuditTrace::new_with_policy_version(
            TraceId::new(16_001),
            SurfaceScope::Cluster,
            CertificateIdentity::new(
                "fp-cli-hadr-controller",
                "CN=andromeda-cli-hadr",
                SurfaceScope::Cluster,
            )?,
            UserPrincipal::new("svc-andromeda-cli-hadr", UserPrincipalKind::Service)?,
            Permission::ClusterPromote,
            SecurityAuditOutcome::Allowed,
            SecurityPolicyVersionEvidence::bootstrap_v0(),
            "authorized HADR primary promotion via CLI",
        )?,
    )
}

fn build_promotion_attempt(
    replica_id: u64,
    options: &super::parsing::PromoteOptions,
    snapshot: &HadrMembershipSnapshot,
) -> AndromedaResult<PromotionAttempt> {
    let candidate_id = HadrNodeId::new(replica_id);
    if candidate_id.is_zero() {
        return Err(cli_error("replica-id must not be zero"));
    }
    let candidate_lsn = required_u64(options.candidate_lsn, "--candidate-lsn")?;
    let audit_lsn = required_u64(options.audit_lsn, "--audit-lsn")?;
    let commit_quorum = options
        .commit_quorum
        .ok_or_else(|| cli_error("hadr promote requires --commit-quorum <count>"))?;
    if commit_quorum == 0 {
        return Err(cli_error("--commit-quorum must be greater than zero"));
    }
    let quorum_size = snapshot.nodes().len() / 2 + 1;
    if commit_quorum < quorum_size {
        return Err(cli_error(format!(
            "--commit-quorum {commit_quorum} is below majority quorum size {quorum_size}"
        )));
    }
    if commit_quorum > snapshot.nodes().len() {
        return Err(cli_error(
            "--commit-quorum cannot exceed durable membership size",
        ));
    }
    if candidate_lsn < audit_lsn {
        return Err(cli_error(
            "--candidate-lsn must be greater than or equal to --audit-lsn",
        ));
    }
    let primary = snapshot.primary().ok_or_else(|| {
        cli_error(
            "hadr promote requires active primary fencing evidence in durable membership store",
        )
    })?;
    if primary.id == candidate_id {
        return Err(cli_error(
            "hadr promote candidate is already the durable primary",
        ));
    }

    let mut voter_ids = Vec::with_capacity(commit_quorum);
    if snapshot.contains(candidate_id) {
        voter_ids.push(candidate_id);
    }
    for node in snapshot.nodes() {
        if voter_ids.len() == commit_quorum {
            break;
        }
        if node.id != candidate_id {
            voter_ids.push(node.id);
        }
    }
    let votes = voter_ids
        .into_iter()
        .map(|node_id| HadrPromotionVote::grant(node_id, snapshot.epoch(), Lsn::new(audit_lsn)))
        .collect();
    let fencing = fencing_context_from_snapshot(snapshot);

    Ok(PromotionAttempt::new(
        candidate_id,
        snapshot.epoch(),
        Lsn::new(candidate_lsn),
        Lsn::new(audit_lsn),
        votes,
        fencing,
    ))
}

fn required_u64(value: Option<u64>, option: &'static str) -> AndromedaResult<u64> {
    value.ok_or_else(|| cli_error(format!("hadr promote requires {option} <lsn>")))
}

fn fencing_context_from_snapshot(snapshot: &HadrMembershipSnapshot) -> HadrFencingContext {
    if let Some(primary) = snapshot.primary() {
        HadrFencingContext::with_active(
            HadrFencingToken::new(primary.id, snapshot.epoch()),
            snapshot.epoch(),
        )
    } else {
        HadrFencingContext {
            active_token: None,
            highest_observed_epoch: snapshot.epoch(),
        }
    }
}
