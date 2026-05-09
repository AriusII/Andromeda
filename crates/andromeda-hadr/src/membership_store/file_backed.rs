use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use andromeda_error::AndromedaResult;

use crate::{
    Lsn,
    membership_persistence_format::{decode_snapshot, encode_snapshot, read_membership_file},
    types::{HadrEpoch, HadrNodeId, HadrNodeRole},
};

use super::{HadrMembershipSnapshot, HadrMembershipStore, io_error, storage_error};

/// File-backed HADR membership store using canonical bytes plus SHA-256.
#[derive(Debug, Clone)]
pub struct FileBackedHadrMembershipStore {
    path: PathBuf,
}

impl FileBackedHadrMembershipStore {
    pub fn open(path: impl Into<PathBuf>) -> AndromedaResult<Self> {
        let store = Self { path: path.into() };
        store.recover_interrupted_publish()?;
        if store.path.exists() {
            store
                .load()?
                .ok_or_else(|| storage_error("HADR membership file is missing"))?;
        }
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn tmp_path(&self) -> PathBuf {
        self.path.with_extension("tmp")
    }

    fn backup_path(&self) -> PathBuf {
        self.path.with_extension("bak")
    }

    fn recover_interrupted_publish(&self) -> AndromedaResult<()> {
        let tmp_path = self.tmp_path();
        let backup_path = self.backup_path();

        if !self.path.exists() && backup_path.exists() {
            fs::rename(&backup_path, &self.path)
                .map_err(|err| io_error("restore HADR membership backup file", err))?;
        }
        if self.path.exists() && backup_path.exists() {
            fs::remove_file(&backup_path)
                .map_err(|err| io_error("remove stale HADR membership backup file", err))?;
        }
        if self.path.exists() && tmp_path.exists() {
            fs::remove_file(&tmp_path)
                .map_err(|err| io_error("remove stale HADR membership temp file", err))?;
        }
        Ok(())
    }

    fn load_or_empty(&self) -> AndromedaResult<HadrMembershipSnapshot> {
        self.load()
            .map(|snapshot| snapshot.unwrap_or_else(HadrMembershipSnapshot::empty))
    }

    fn persist(&self, snapshot: &HadrMembershipSnapshot) -> AndromedaResult<()> {
        snapshot.validate()?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| io_error("create HADR membership directory", err))?;
        }

        let bytes = encode_snapshot(snapshot)?;
        let tmp_path = self.tmp_path();
        let backup_path = self.backup_path();
        let mut tmp = File::create(&tmp_path)
            .map_err(|err| io_error("create HADR membership temp file", err))?;
        tmp.write_all(&bytes)
            .map_err(|err| io_error("write HADR membership temp file", err))?;
        tmp.sync_all()
            .map_err(|err| io_error("sync HADR membership temp file", err))?;
        drop(tmp);

        if backup_path.exists() {
            fs::remove_file(&backup_path)
                .map_err(|err| io_error("remove stale HADR membership backup file", err))?;
        }
        if self.path.exists() {
            fs::rename(&self.path, &backup_path)
                .map_err(|err| io_error("backup HADR membership file", err))?;
        }
        if let Err(err) = fs::rename(&tmp_path, &self.path) {
            if backup_path.exists() && !self.path.exists() {
                let _ = fs::rename(&backup_path, &self.path);
            }
            return Err(io_error("rename HADR membership temp file", err));
        }

        if backup_path.exists() {
            let _ = fs::remove_file(&backup_path);
        }

        Ok(())
    }
}

impl HadrMembershipStore for FileBackedHadrMembershipStore {
    fn load(&self) -> AndromedaResult<Option<HadrMembershipSnapshot>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = read_membership_file(&self.path)?;
        decode_snapshot(&bytes).map(Some)
    }

    fn register_node(
        &self,
        id: HadrNodeId,
        role: HadrNodeRole,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot = self.load_or_empty()?.register_node(id, role)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn update_node_role(
        &self,
        id: HadrNodeId,
        role: HadrNodeRole,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot = self.load_or_empty()?.update_node_role(id, role)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn deregister_node(&self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot = self.load_or_empty()?.deregister_node(id)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn fence_node(&self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot = self.load_or_empty()?.fence_node(id)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn advance_epoch(&self, target_epoch: HadrEpoch) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot = self.load_or_empty()?.advance_epoch(target_epoch)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn promote_primary(
        &self,
        id: HadrNodeId,
        target_epoch: HadrEpoch,
        committed_safe_lsn: Lsn,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot =
            self.load_or_empty()?
                .promote_primary(id, target_epoch, committed_safe_lsn)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }
}
