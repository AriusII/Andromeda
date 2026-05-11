use std::fs;

mod support;

use andromeda_hadr::{
    FileBackedHadrMembershipStore, HadrMembershipStore, HadrNodeId, HadrNodeRole,
};

fn store_path(dir: &support::TempDir) -> std::path::PathBuf {
    dir.path().join("hadr-membership.store")
}

fn publish_or_skip_windows(store: &FileBackedHadrMembershipStore) -> bool {
    match store.register_node(HadrNodeId::new(1), HadrNodeRole::Primary) {
        Ok(_) => true,
        Err(error) if cfg!(windows) => {
            assert!(
                error.message().contains("sync HADR membership directory"),
                "unexpected windows publish error: {error}"
            );
            false
        },
        Err(error) => panic!("publish failed: {error}"),
    }
}

#[test]
fn publish_leaves_only_final_file_on_success() {
    let dir = support::tempdir("membership-durability").expect("temp dir");
    let path = store_path(&dir);
    let store = FileBackedHadrMembershipStore::open(&path).expect("open store");
    if !publish_or_skip_windows(&store) {
        return;
    }

    assert!(path.exists(), "final membership file must exist");
    assert!(
        !path.with_extension("tmp").exists(),
        "temp file must not remain"
    );
    assert!(
        !path.with_extension("bak").exists(),
        "backup file must not remain after commit"
    );
}

#[test]
fn open_recovers_backup_when_final_missing_and_removes_tmp() {
    let dir = support::tempdir("membership-durability").expect("temp dir");
    let path = store_path(&dir);
    let store = FileBackedHadrMembershipStore::open(&path).expect("open store");
    if !publish_or_skip_windows(&store) {
        return;
    }
    let before = store.load().expect("load snapshot").expect("snapshot");

    let backup_path = path.with_extension("bak");
    let tmp_path = path.with_extension("tmp");
    fs::rename(&path, &backup_path).expect("move final to backup");
    fs::write(&tmp_path, b"incomplete-new-state").expect("write temp");

    let reopened = FileBackedHadrMembershipStore::open(&path).expect("open recovered");
    let loaded = reopened.load().expect("load recovered").expect("snapshot");
    assert_eq!(loaded, before);
    assert!(path.exists(), "final file must be restored");
    assert!(!backup_path.exists(), "backup must be cleaned");
    assert!(!tmp_path.exists(), "temp must be cleaned");
}

#[test]
fn open_removes_stale_backup_when_final_exists() {
    let dir = support::tempdir("membership-durability").expect("temp dir");
    let path = store_path(&dir);
    let store = FileBackedHadrMembershipStore::open(&path).expect("open store");
    if !publish_or_skip_windows(&store) {
        return;
    }

    let backup_path = path.with_extension("bak");
    fs::copy(&path, &backup_path).expect("copy stale backup");

    let reopened = FileBackedHadrMembershipStore::open(&path).expect("open store");
    let _ = reopened.load().expect("load snapshot").expect("snapshot");
    assert!(path.exists(), "final file must remain");
    assert!(!backup_path.exists(), "stale backup must be removed");
}
