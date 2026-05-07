use andromeda_storage::{
    Lsn, validate_lsn_strictly_ordered, validate_manifest_atomic_switch, validate_recovery_floor,
    validate_wal_durability_before_page_flush,
};

pub(crate) fn assert_page_flush_allowed(page_lsn: Lsn, wal_durable: Lsn, context: &str) {
    assert!(
        validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_ok(),
        "{context}"
    );
}

pub(crate) fn assert_page_flush_blocked(page_lsn: Lsn, wal_durable: Lsn, context: &str) {
    assert!(
        validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_err(),
        "{context}"
    );
}

pub(crate) fn assert_manifest_switch_allowed(
    manifest_checkpoint_lsn: Lsn,
    wal_durable_lsn: Lsn,
    wal_checkpoint_lsn: Lsn,
    context: &str,
) {
    assert!(
        validate_manifest_atomic_switch(
            manifest_checkpoint_lsn,
            wal_durable_lsn,
            wal_checkpoint_lsn,
        )
        .is_ok(),
        "{context}"
    );
}

pub(crate) fn assert_recovery_floor_allowed(
    recovery_floor_lsn: Lsn,
    manifest_required_wal_start: Lsn,
    context: &str,
) {
    assert!(
        validate_recovery_floor(recovery_floor_lsn, manifest_required_wal_start).is_ok(),
        "{context}"
    );
}

pub(crate) fn assert_lsn_strictly_ordered(
    earlier_lsn: Lsn,
    earlier_label: &str,
    later_lsn: Lsn,
    later_label: &str,
    context: &str,
) {
    assert!(
        validate_lsn_strictly_ordered(earlier_lsn, earlier_label, later_lsn, later_label).is_ok(),
        "{context}"
    );
}

pub(crate) fn assert_message_contains(message: &str, fragment: &str, context: &str) {
    assert!(message.contains(fragment), "{context}");
}
