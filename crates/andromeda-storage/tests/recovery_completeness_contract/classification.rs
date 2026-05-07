use super::support::{
    ALL_WAL_RECORD_KINDS, FUTURE_WORK_KINDS, SKIPPED_OR_IMPLEMENTED_KINDS,
    is_index_btree_recovery_kind, record_for_kind,
};
use andromeda_storage::{Lsn, ReplayContext, ReplayOutcome, WalRecordKind, replay_wal_record};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryPromotionFamily {
    PageLifecycle,
    AccessPathRebuild,
    MvccVersion,
    MapDelta,
    CatalogDefinitionBatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RecoveryPromotionGate {
    kind: WalRecordKind,
    family: RecoveryPromotionFamily,
    payload_codec_required: bool,
    golden_vectors_required: bool,
    property_or_fuzz_required: bool,
    crash_recovery_required: bool,
    inline_replay_allowed_before_gate: bool,
}

const FUTURE_WORK_PROMOTION_GATES: [RecoveryPromotionGate; 14] = [
    gate(
        WalRecordKind::PageAllocate,
        RecoveryPromotionFamily::PageLifecycle,
    ),
    gate(
        WalRecordKind::PageFormat,
        RecoveryPromotionFamily::PageLifecycle,
    ),
    gate(
        WalRecordKind::IndexInsert,
        RecoveryPromotionFamily::AccessPathRebuild,
    ),
    gate(
        WalRecordKind::IndexDelete,
        RecoveryPromotionFamily::AccessPathRebuild,
    ),
    gate(
        WalRecordKind::MvccVersionCreate,
        RecoveryPromotionFamily::MvccVersion,
    ),
    gate(
        WalRecordKind::MvccVersionClose,
        RecoveryPromotionFamily::MvccVersion,
    ),
    gate(
        WalRecordKind::MapDeltaAppend,
        RecoveryPromotionFamily::MapDelta,
    ),
    gate(
        WalRecordKind::CatalogChangeBegin,
        RecoveryPromotionFamily::CatalogDefinitionBatch,
    ),
    gate(
        WalRecordKind::CatalogChangeApply,
        RecoveryPromotionFamily::CatalogDefinitionBatch,
    ),
    gate(
        WalRecordKind::CatalogChangeCommit,
        RecoveryPromotionFamily::CatalogDefinitionBatch,
    ),
    gate(
        WalRecordKind::BTreeInsert,
        RecoveryPromotionFamily::AccessPathRebuild,
    ),
    gate(
        WalRecordKind::BTreeDelete,
        RecoveryPromotionFamily::AccessPathRebuild,
    ),
    gate(
        WalRecordKind::BTreeSplit,
        RecoveryPromotionFamily::AccessPathRebuild,
    ),
    gate(
        WalRecordKind::BTreeMerge,
        RecoveryPromotionFamily::AccessPathRebuild,
    ),
];

const fn gate(kind: WalRecordKind, family: RecoveryPromotionFamily) -> RecoveryPromotionGate {
    RecoveryPromotionGate {
        kind,
        family,
        payload_codec_required: true,
        golden_vectors_required: true,
        property_or_fuzz_required: true,
        crash_recovery_required: true,
        inline_replay_allowed_before_gate: false,
    }
}

#[test]
fn all_record_kinds_are_classified_exactly_once() {
    assert_eq!(ALL_WAL_RECORD_KINDS.len(), 26);
    assert_eq!(SKIPPED_OR_IMPLEMENTED_KINDS.len(), 12);
    assert_eq!(FUTURE_WORK_KINDS.len(), 14);

    for kind in &ALL_WAL_RECORD_KINDS {
        let kind_name = format!("{kind:?}");
        let handled = SKIPPED_OR_IMPLEMENTED_KINDS.contains(kind);
        let deferred = FUTURE_WORK_KINDS.contains(kind);
        assert_ne!(
            handled, deferred,
            "{kind_name} must be classified as exactly one recovery category",
        );
    }
}

#[test]
fn future_work_records_have_explicit_promotion_gates() {
    assert_eq!(FUTURE_WORK_PROMOTION_GATES.len(), FUTURE_WORK_KINDS.len());

    for kind in FUTURE_WORK_KINDS {
        let matches: Vec<_> = FUTURE_WORK_PROMOTION_GATES
            .iter()
            .filter(|gate| gate.kind == kind)
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "{kind:?} must have exactly one recovery promotion gate",
        );

        let gate = matches[0];
        assert_eq!(
            gate.family,
            expected_promotion_family(kind),
            "{kind:?} must stay in its documented recovery promotion family",
        );
        assert!(
            gate.payload_codec_required,
            "{kind:?} must define an explicit payload codec before replay promotion",
        );
        assert!(
            gate.golden_vectors_required,
            "{kind:?} must add golden vectors before replay promotion",
        );
        assert!(
            gate.property_or_fuzz_required,
            "{kind:?} must add property or fuzz coverage before replay promotion",
        );
        assert!(
            gate.crash_recovery_required,
            "{kind:?} must add crash/recovery coverage before replay promotion",
        );
        assert!(
            !gate.inline_replay_allowed_before_gate,
            "{kind:?} must remain fail-stop until every promotion gate is satisfied",
        );
    }
}

fn expected_promotion_family(kind: WalRecordKind) -> RecoveryPromotionFamily {
    match kind {
        WalRecordKind::PageAllocate | WalRecordKind::PageFormat => {
            RecoveryPromotionFamily::PageLifecycle
        }
        WalRecordKind::IndexInsert
        | WalRecordKind::IndexDelete
        | WalRecordKind::BTreeInsert
        | WalRecordKind::BTreeDelete
        | WalRecordKind::BTreeSplit
        | WalRecordKind::BTreeMerge => RecoveryPromotionFamily::AccessPathRebuild,
        WalRecordKind::MvccVersionCreate | WalRecordKind::MvccVersionClose => {
            RecoveryPromotionFamily::MvccVersion
        }
        WalRecordKind::MapDeltaAppend => RecoveryPromotionFamily::MapDelta,
        WalRecordKind::CatalogChangeBegin
        | WalRecordKind::CatalogChangeApply
        | WalRecordKind::CatalogChangeCommit => RecoveryPromotionFamily::CatalogDefinitionBatch,
        other => panic!("{other:?} is not a deferred recovery promotion kind"),
    }
}

#[test]
fn marker_and_boundary_records_are_skipped_without_errors() {
    let skipped_kinds = [
        WalRecordKind::TxBegin,
        WalRecordKind::TxCommit,
        WalRecordKind::TxRollback,
        WalRecordKind::CheckpointBegin,
        WalRecordKind::CheckpointEnd,
        WalRecordKind::SnapshotBegin,
        WalRecordKind::SnapshotEnd,
        WalRecordKind::SecurityAuditAppend,
    ];

    let mut ctx = ReplayContext::new();
    for (idx, kind) in skipped_kinds.iter().copied().enumerate() {
        let record = record_for_kind(kind, Lsn::new(idx as u64 + 1));
        replay_wal_record(&mut ctx, &record).expect("skipped handler should not fail");
    }

    assert_eq!(ctx.applied_count, 0);
    assert_eq!(ctx.skipped_count, skipped_kinds.len());
    assert!(!ctx.has_errors());
    assert_eq!(
        ctx.last_replayed_lsn,
        Some(Lsn::new(skipped_kinds.len() as u64))
    );
}

#[test]
fn future_work_records_fail_stop_with_clear_error_and_context() {
    for (idx, kind) in FUTURE_WORK_KINDS.into_iter().enumerate() {
        let mut ctx = ReplayContext::new();
        let record = record_for_kind(kind, Lsn::new(idx as u64 + 10));
        let err =
            replay_wal_record(&mut ctx, &record).expect_err("future work handlers must fail-stop");
        let message = err.message();

        if is_index_btree_recovery_kind(kind) {
            assert!(
                message.contains("malformed"),
                "{kind:?} error must identify malformed rebuild evidence payloads"
            );
        } else {
            assert!(
                message.contains("not promoted"),
                "{kind:?} error must identify the recovery promotion gate"
            );
        }
        assert!(
            message.contains(&format!("{kind:?}")),
            "{kind:?} error must include record kind"
        );
        assert!(ctx.has_errors());
        assert_eq!(ctx.error_records.len(), 1);
        assert_eq!(ctx.error_records[0].kind, kind);
        assert_eq!(
            ctx.error_records[0].outcome,
            ReplayOutcome::NotYetImplemented
        );
    }
}
