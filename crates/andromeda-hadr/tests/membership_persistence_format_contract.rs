use std::fs;

mod support;

use andromeda_hadr::{
    FileBackedHadrMembershipStore, HadrMembershipStore, HadrNodeId, HadrNodeRole,
};
use sha2::{Digest, Sha256};

const HEADER_LEN: usize = 56;
const PAYLOAD_VERSION_OFFSET: usize = HEADER_LEN;
const PAYLOAD_NODE_COUNT_OFFSET: usize = HEADER_LEN + 16;
const PAYLOAD_RECORD_COUNT_OFFSET: usize = HEADER_LEN + 24;
const PAYLOAD_LEN_FIELD_OFFSET: usize = 48;
const FILE_MAX_BYTES: usize = 1024 * 1024;

fn store_path(dir: &support::TempDir) -> std::path::PathBuf {
    dir.path().join("hadr-membership.store")
}

fn build_fixture() -> Option<(support::TempDir, std::path::PathBuf)> {
    let dir = support::tempdir("membership-format").expect("temp dir");
    let path = store_path(&dir);
    let store = FileBackedHadrMembershipStore::open(&path).expect("open store");
    match store.register_node(HadrNodeId::new(1), HadrNodeRole::Primary) {
        Ok(_) => Some((dir, path)),
        Err(error) if cfg!(windows) => {
            assert!(
                error.message().contains("sync HADR membership directory"),
                "unexpected windows publish error: {error}"
            );
            None
        },
        Err(error) => panic!("publish failed: {error}"),
    }
}

fn rewrite_checksum(bytes: &mut [u8]) {
    let checksum = Sha256::digest(&bytes[HEADER_LEN..]);
    bytes[16..48].copy_from_slice(&checksum);
}

fn rewrite_payload_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    rewrite_checksum(bytes);
}

#[test]
fn format_rejects_magic_mismatch() {
    let Some((_dir, path)) = build_fixture() else {
        return;
    };
    let mut bytes = fs::read(&path).expect("read membership file");
    bytes[0] ^= 0xFF;
    fs::write(&path, bytes).expect("write corrupted magic");

    let error = FileBackedHadrMembershipStore::open(&path).expect_err("open must fail");
    assert!(error.message().contains("magic mismatch"));
}

#[test]
fn format_rejects_unsupported_version() {
    let Some((_dir, path)) = build_fixture() else {
        return;
    };
    let mut bytes = fs::read(&path).expect("read membership file");
    rewrite_payload_u64(&mut bytes, PAYLOAD_VERSION_OFFSET, 99);
    fs::write(&path, bytes).expect("write bad version");

    let error = FileBackedHadrMembershipStore::open(&path).expect_err("open must fail");
    assert!(error.message().contains("version is unsupported"));
}

#[test]
fn format_rejects_payload_length_mismatch() {
    let Some((_dir, path)) = build_fixture() else {
        return;
    };
    let mut bytes = fs::read(&path).expect("read membership file");
    let declared = u64::from_le_bytes(
        bytes[PAYLOAD_LEN_FIELD_OFFSET..PAYLOAD_LEN_FIELD_OFFSET + 8]
            .try_into()
            .expect("payload length field"),
    );
    bytes[PAYLOAD_LEN_FIELD_OFFSET..PAYLOAD_LEN_FIELD_OFFSET + 8]
        .copy_from_slice(&(declared + 1).to_le_bytes());
    fs::write(&path, bytes).expect("write payload length mismatch");

    let error = FileBackedHadrMembershipStore::open(&path).expect_err("open must fail");
    assert!(error.message().contains("file length mismatch"));
}

#[test]
fn format_rejects_node_count_above_bound() {
    let Some((_dir, path)) = build_fixture() else {
        return;
    };
    let mut bytes = fs::read(&path).expect("read membership file");
    rewrite_payload_u64(&mut bytes, PAYLOAD_NODE_COUNT_OFFSET, 1_025);
    fs::write(&path, bytes).expect("write oversized node count");

    let error = FileBackedHadrMembershipStore::open(&path).expect_err("open must fail");
    assert!(error.message().contains("node count exceeds bounded limit"));
}

#[test]
fn format_rejects_record_count_above_bound() {
    let Some((_dir, path)) = build_fixture() else {
        return;
    };
    let mut bytes = fs::read(&path).expect("read membership file");
    rewrite_payload_u64(&mut bytes, PAYLOAD_RECORD_COUNT_OFFSET, 8_193);
    fs::write(&path, bytes).expect("write oversized record count");

    let error = FileBackedHadrMembershipStore::open(&path).expect_err("open must fail");
    assert!(
        error
            .message()
            .contains("record count exceeds bounded limit")
    );
}

#[test]
fn format_rejects_oversized_file_before_decode() {
    let dir = support::tempdir("membership-format").expect("temp dir");
    let path = store_path(&dir);
    fs::write(&path, vec![0_u8; FILE_MAX_BYTES + 1]).expect("write oversized file");

    let error = FileBackedHadrMembershipStore::open(&path).expect_err("open must fail");
    assert!(error.message().contains("exceeds bounded read limit"));
}

#[test]
fn format_rejects_truncated_file() {
    let dir = support::tempdir("membership-format").expect("temp dir");
    let path = store_path(&dir);
    fs::write(&path, vec![0_u8; HEADER_LEN - 1]).expect("write truncated file");

    let error = FileBackedHadrMembershipStore::open(&path).expect_err("open must fail");
    assert!(error.message().contains("is truncated"));
}
