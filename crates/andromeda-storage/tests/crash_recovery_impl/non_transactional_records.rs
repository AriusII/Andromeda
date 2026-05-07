use andromeda_storage::{RedoRecordDecision, WalRecordKind};

use crate::fixtures::{assert_decision, non_transactional_redo, plan_at_lsn1};

fn assert_non_transactional_replays(kind: WalRecordKind, payload: &'static [u8]) {
    let records = vec![non_transactional_redo(kind, 1, None, payload)];
    let plan = plan_at_lsn1(&records);
    assert_decision(&plan, 1, RedoRecordDecision::Replay);
}

// CAC-31 ManifestSwitch, non-transactional: always Replay.
#[test]
fn test_cac_31_manifest_switch_always_replay() {
    assert_non_transactional_replays(WalRecordKind::ManifestSwitch, b"ms");
}

// CAC-32 PageAllocate, non-transactional: always Replay.
#[test]
fn test_cac_32_page_allocate_always_replay() {
    assert_non_transactional_replays(WalRecordKind::PageAllocate, b"pa");
}

// CAC-33 PageFormat, non-transactional: always Replay.
#[test]
fn test_cac_33_page_format_always_replay() {
    assert_non_transactional_replays(WalRecordKind::PageFormat, b"pf");
}

// CAC-34 MapDeltaAppend, non-transactional: always Replay.
#[test]
fn test_cac_34_map_delta_append_always_replay() {
    assert_non_transactional_replays(WalRecordKind::MapDeltaAppend, b"md");
}

// CAC-35 SecurityAuditAppend, non-transactional: always Replay.
#[test]
fn test_cac_35_security_audit_always_replay() {
    assert_non_transactional_replays(WalRecordKind::SecurityAuditAppend, b"sa");
}
