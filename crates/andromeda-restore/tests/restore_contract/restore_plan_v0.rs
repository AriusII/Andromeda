use crate::support::*;
use andromeda_restore::{
    RecoveryStage, RestoreCompletion, RestorePlanCompletionEvidenceV0, RestorePlanError,
    RestorePlanV0, RestoreValidationPolicy, WalSegmentToReplay, plan_replay_segments,
};
use andromeda_wal::Lsn;

fn build_codec_plan() -> RestorePlanV0 {
    let manifest = make_test_manifest();
    let pitr_target = Lsn::new(1500);
    let replay_plan = plan_replay_segments(
        &manifest,
        pitr_target,
        &[
            make_wal_segment(1001, 1200, None),
            make_wal_segment(1201, 2000, Some(1200)),
        ],
    )
    .expect("replay plan should be valid");
    let audit = restore_audit_with_checksum(
        manifest.backup_id,
        pitr_target,
        RecoveryStage::SafeStart,
        42,
    );
    let orchestration = restore_orchestration_for(
        manifest,
        pitr_target,
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );
    RestorePlanV0::from_orchestration(&orchestration, &replay_plan, 42)
        .expect("restore plan should construct")
}

#[test]
fn restore_plan_v0_requires_explicit_non_zero_pitr_target() {
    let manifest = make_test_manifest();
    let audit = restore_audit_for(&manifest, Lsn::new(0), RecoveryStage::SafeStart);
    let orchestration = restore_orchestration_for(
        manifest,
        Lsn::new(0),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = RestorePlanV0::from_orchestration(&orchestration, &[], 7)
        .expect_err("restore plan must reject implicit/zero PITR target");
    assert_eq!(err, RestorePlanError::ExplicitPitrTargetRequired);
}

#[test]
fn restore_plan_v0_binds_completion_to_plan_identity_target_and_stop() {
    let manifest = make_test_manifest();
    let pitr_target = Lsn::new(1500);
    let replay_plan = plan_replay_segments(
        &manifest,
        pitr_target,
        &[make_wal_segment(1001, 2000, None)],
    )
    .expect("replay plan should be valid");
    let audit = restore_audit_with_checksum(
        manifest.backup_id,
        pitr_target,
        RecoveryStage::SafeStart,
        42,
    );
    let orchestration = restore_orchestration_for(
        manifest,
        pitr_target,
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );
    let plan = RestorePlanV0::from_orchestration(&orchestration, &replay_plan, 42)
        .expect("restore plan should construct");

    let completion = RestoreCompletion::Success {
        replayed_lsn: pitr_target,
        final_checkpoint_lsn: pitr_target,
    };
    let evidence = RestorePlanCompletionEvidenceV0::bind(&plan, completion, 42)
        .expect("completion evidence should bind to plan");

    assert_eq!(evidence.plan_identity, plan.plan_identity());
    assert_eq!(evidence.target_lsn, plan.pitr_target_lsn());
    assert_eq!(evidence.replay_stop_lsn, plan.replay_stop_lsn());
}

#[test]
fn restore_plan_v0_rejects_missing_replay_segment_coverage() {
    let manifest = make_test_manifest();
    let pitr_target = Lsn::new(1500);
    let replay_plan = vec![WalSegmentToReplay {
        segment_descriptor: make_wal_segment(1001, 1200, None),
        sequence_index: 0,
        contains_pitr_target: false,
        replay_stop_lsn: Lsn::new(1200),
    }];
    let audit = restore_audit_with_checksum(
        manifest.backup_id,
        pitr_target,
        RecoveryStage::SafeStart,
        99,
    );
    let orchestration = restore_orchestration_for(
        manifest,
        pitr_target,
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = RestorePlanV0::from_orchestration(&orchestration, &replay_plan, 99)
        .expect_err("restore plan must reject replay segments that miss PITR target");
    assert_eq!(err, RestorePlanError::MissingReplayCoverage);
}

#[test]
fn restore_plan_v0_rejects_manifest_bound_audit_with_arbitrary_preflight_checksum() {
    let manifest = make_test_manifest();
    let pitr_target = Lsn::new(1500);
    let replay_plan = plan_replay_segments(
        &manifest,
        pitr_target,
        &[make_wal_segment(1001, 2000, None)],
    )
    .expect("replay plan should be valid");
    let audit = restore_audit_for(&manifest, pitr_target, RecoveryStage::SafeStart);
    let orchestration = restore_orchestration_for(
        manifest,
        pitr_target,
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = RestorePlanV0::from_orchestration(&orchestration, &replay_plan, 8)
        .expect_err("restore plan must require durable preflight-bound audit evidence");
    assert!(matches!(err, RestorePlanError::OrchestrationInvalid { .. }));
    assert!(
        err.to_string().contains("preflight evidence checksum"),
        "unexpected error: {err}"
    );
}

#[test]
fn restore_plan_v0_rejects_gapped_replay_segment_summary() {
    let manifest = make_test_manifest();
    let pitr_target = Lsn::new(1500);
    let replay_plan = vec![
        WalSegmentToReplay {
            segment_descriptor: make_wal_segment(1001, 1200, None),
            sequence_index: 0,
            contains_pitr_target: false,
            replay_stop_lsn: Lsn::new(1200),
        },
        WalSegmentToReplay {
            segment_descriptor: make_wal_segment(1300, 1500, Some(1200)),
            sequence_index: 1,
            contains_pitr_target: true,
            replay_stop_lsn: pitr_target,
        },
    ];
    let audit = restore_audit_with_checksum(
        manifest.backup_id,
        pitr_target,
        RecoveryStage::SafeStart,
        99,
    );
    let orchestration = restore_orchestration_for(
        manifest,
        pitr_target,
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = RestorePlanV0::from_orchestration(&orchestration, &replay_plan, 99)
        .expect_err("restore plan must reject WAL replay gaps");
    assert_eq!(err, RestorePlanError::ReplaySegmentChainInvalid);
}

#[test]
fn restore_plan_v0_rejects_non_tail_segment_marked_as_target() {
    let manifest = make_test_manifest();
    let pitr_target = Lsn::new(1500);
    let replay_plan = vec![
        WalSegmentToReplay {
            segment_descriptor: make_wal_segment(1001, 1200, None),
            sequence_index: 0,
            contains_pitr_target: true,
            replay_stop_lsn: Lsn::new(1200),
        },
        WalSegmentToReplay {
            segment_descriptor: make_wal_segment(1201, 1500, Some(1200)),
            sequence_index: 1,
            contains_pitr_target: true,
            replay_stop_lsn: pitr_target,
        },
    ];
    let audit = restore_audit_with_checksum(
        manifest.backup_id,
        pitr_target,
        RecoveryStage::SafeStart,
        99,
    );
    let orchestration = restore_orchestration_for(
        manifest,
        pitr_target,
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = RestorePlanV0::from_orchestration(&orchestration, &replay_plan, 99)
        .expect_err("restore plan must reject ambiguous target segment marking");
    assert_eq!(err, RestorePlanError::MissingReplayCoverage);
}

#[test]
fn restore_plan_completion_evidence_rejects_preflight_checksum_mismatch() {
    let manifest = make_test_manifest();
    let pitr_target = Lsn::new(1500);
    let replay_plan = plan_replay_segments(
        &manifest,
        pitr_target,
        &[make_wal_segment(1001, 2000, None)],
    )
    .expect("replay plan should be valid");
    let audit =
        restore_audit_with_checksum(manifest.backup_id, pitr_target, RecoveryStage::SafeStart, 8);
    let orchestration = restore_orchestration_for(
        manifest,
        pitr_target,
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );
    let plan = RestorePlanV0::from_orchestration(&orchestration, &replay_plan, 8)
        .expect("restore plan should construct");
    let completion = RestoreCompletion::Success {
        replayed_lsn: pitr_target,
        final_checkpoint_lsn: pitr_target,
    };

    let err = RestorePlanCompletionEvidenceV0::bind(&plan, completion, 9)
        .expect_err("completion evidence must bind the preflight checksum");
    assert_eq!(err, RestorePlanError::PreflightChecksumMismatch);
}

#[test]
fn restore_plan_v0_codec_roundtrip_preserves_all_fields() {
    let plan = build_codec_plan();
    let encoded = plan.encode_binary_v0();
    let decoded = RestorePlanV0::decode_binary_v0(&encoded).expect("decode should succeed");

    assert_eq!(decoded, plan);
}

#[test]
fn restore_plan_v0_codec_rejects_truncated_trailing_and_corrupt_payloads() {
    let plan = build_codec_plan();
    let encoded = plan.encode_binary_v0();

    let truncated = &encoded[..encoded.len() - 1];
    let err =
        RestorePlanV0::decode_binary_v0(truncated).expect_err("truncated payload must fail closed");
    assert_eq!(err, RestorePlanError::PlanCodecTruncated);

    let mut trailing = encoded.clone();
    trailing.push(0xFF);
    let err =
        RestorePlanV0::decode_binary_v0(&trailing).expect_err("trailing bytes must fail closed");
    assert_eq!(err, RestorePlanError::PlanCodecTrailingBytes);

    let mut corrupted = encoded;
    let identity_offset = corrupted.len() - std::mem::size_of::<u64>();
    corrupted[identity_offset] ^= 0x01;
    let err = RestorePlanV0::decode_binary_v0(&corrupted)
        .expect_err("corrupted identity must fail closed");
    assert_eq!(err, RestorePlanError::PlanIdentityMismatch);
}

#[test]
fn restore_plan_v0_codec_rejects_invalid_enum_discriminants() {
    let plan = build_codec_plan();
    let mut encoded = plan.encode_binary_v0();

    let stage_offset = encoded.len() - (std::mem::size_of::<u64>() + 2);
    encoded[stage_offset] = 9;
    let err = RestorePlanV0::decode_binary_v0(&encoded)
        .expect_err("invalid recovery stage must fail decode");
    assert_eq!(
        err,
        RestorePlanError::PlanCodecInvalidEnum {
            field: "recovery_stage",
            value: 9,
        }
    );
}

#[test]
fn restore_plan_v0_codec_rejects_unbounded_segment_count_before_allocation() {
    let plan = build_codec_plan();
    let mut encoded = plan.encode_binary_v0();
    let replay_segment_count_offset =
        8 + std::mem::size_of::<u16>() + (3 * std::mem::size_of::<u64>());
    encoded[replay_segment_count_offset..replay_segment_count_offset + std::mem::size_of::<u64>()]
        .copy_from_slice(&u64::MAX.to_le_bytes());

    let err = RestorePlanV0::decode_binary_v0(&encoded)
        .expect_err("untrusted segment count must fail before allocation");
    assert!(matches!(
        err,
        RestorePlanError::PlanCodecValueInvalid {
            field: "replay_segment_count",
            ..
        }
    ));
}

#[test]
fn restore_plan_v0_codec_rejects_unknown_version() {
    let plan = build_codec_plan();
    let mut encoded = plan.encode_binary_v0();
    encoded[8] = 2;
    encoded[9] = 0;

    let err =
        RestorePlanV0::decode_binary_v0(&encoded).expect_err("unknown codec version must reject");
    assert_eq!(
        err,
        RestorePlanError::PlanCodecUnsupportedVersion { version: 2 }
    );
}

#[test]
fn restore_plan_v0_codec_has_deterministic_golden_vector() {
    let plan = build_codec_plan();
    let encoded = plan.encode_binary_v0();

    let expected = vec![
        65, 82, 80, 76, 65, 78, 86, 48, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 51, 159, 192, 169, 231, 237,
        60, 253, 220, 5, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 233, 3,
        0, 0, 0, 0, 0, 0, 176, 4, 0, 0, 0, 0, 0, 0, 176, 4, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0,
        0, 0, 177, 4, 0, 0, 0, 0, 0, 0, 208, 7, 0, 0, 0, 0, 0, 0, 220, 5, 0, 0, 0, 0, 0, 0, 1, 220,
        5, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 1, 1, 183, 142, 24, 202, 141, 197, 176, 79,
    ];

    assert_eq!(encoded, expected);
}
