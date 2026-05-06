use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_storage::{
    FileBackedHadrMembershipStore, HadrMembershipSnapshot, HadrMembershipStore, HadrNodeRole,
    HadrPromotionAuditLog, HadrPromotionAuditMarker,
};

use super::types::NodeMembershipMemberReport;

pub(super) fn load_membership_snapshot(
    path: &Path,
) -> AndromedaResult<Option<HadrMembershipSnapshot>> {
    FileBackedHadrMembershipStore::open(path)?.load()
}

pub(super) fn open_membership_store(path: &Path) -> AndromedaResult<FileBackedHadrMembershipStore> {
    FileBackedHadrMembershipStore::open(path)
}

#[derive(Debug, Clone)]
pub(super) struct FileBackedPromotionAuditLog {
    path: PathBuf,
}

impl FileBackedPromotionAuditLog {
    pub(super) fn open(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl HadrPromotionAuditLog for FileBackedPromotionAuditLog {
    fn append_primary_promotion_marker(
        &self,
        marker: &HadrPromotionAuditMarker,
    ) -> AndromedaResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| io_error("create HADR promotion audit directory", err))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|err| io_error("open HADR promotion audit log", err))?;
        writeln!(
            file,
            "{{\"event\":\"hadr.promotion.primary.marker\",\"candidate_id\":{},\"proposed_epoch\":{},\"primary_durable_lsn\":{},\"committed_safe_lsn\":{},\"token_primary_id\":{},\"token_epoch\":{},\"granted_votes\":{},\"quorum_size\":{}}}",
            marker.candidate_id.get(),
            marker.proposed_epoch.get(),
            marker.primary_durable_lsn.get(),
            marker.committed_safe_lsn.get(),
            marker.token.primary_id.get(),
            marker.token.epoch.get(),
            marker.audit_record.granted_votes,
            marker.audit_record.quorum_size,
        )
        .map_err(|err| io_error("write HADR promotion audit marker", err))?;
        file.sync_all()
            .map_err(|err| io_error("sync HADR promotion audit marker", err))?;
        Ok(())
    }
}

pub(super) fn default_promotion_audit_log_path(membership_store: &Path) -> PathBuf {
    let mut path = membership_store.to_path_buf();
    let Some(file_name) = membership_store.file_name() else {
        return path.with_extension("promotion-audit.jsonl");
    };
    path.set_file_name(format!(
        "{}.promotion-audit.jsonl",
        file_name.to_string_lossy()
    ));
    path
}

pub(super) fn role_label(role: HadrNodeRole) -> &'static str {
    match role {
        HadrNodeRole::Primary => "primary",
        HadrNodeRole::Replica => "replica",
        HadrNodeRole::Candidate => "candidate",
    }
}

pub(super) fn quorum_size(total_members: usize) -> usize {
    if total_members == 0 {
        0
    } else {
        total_members / 2 + 1
    }
}

pub(super) fn member_reports(
    snapshot: Option<&HadrMembershipSnapshot>,
) -> Vec<NodeMembershipMemberReport> {
    snapshot
        .into_iter()
        .flat_map(HadrMembershipSnapshot::nodes)
        .map(|node| NodeMembershipMemberReport {
            node_id: node.id.get(),
            role: role_label(node.role).to_string(),
            role_epoch: node.role_epoch.get(),
        })
        .collect()
}

pub(super) fn membership_epoch(snapshot: Option<&HadrMembershipSnapshot>) -> u64 {
    snapshot.map_or(0, |snapshot| snapshot.epoch().get())
}

fn io_error(action: &str, err: std::io::Error) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, format!("{action}: {err}"))
}
