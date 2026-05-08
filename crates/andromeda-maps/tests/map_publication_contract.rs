use andromeda_maps::{
    MapDescriptor, MapDescriptorError, MapGrain, MapId, MapPublicationCandidate,
    MapPublicationEvidence, MapPublicationRebuildEvidence, MapPublicationRecoveryEvidence,
    MapPublicationRollbackEvidence, MapPublicationState, MapPublicationSwitch, MapRefreshMode,
    MapStalenessPolicy,
};

fn descriptor() -> MapDescriptor {
    let id = match MapId::new(7) {
        Ok(id) => id,
        Err(error) => panic!("unexpected map id error: {error}"),
    };

    MapDescriptor::new(
        id,
        MapGrain::Relation,
        MapRefreshMode::Deferred,
        MapStalenessPolicy::CurrentOnly,
    )
}

fn digest() -> [u8; 32] {
    let mut digest = [0_u8; 32];
    digest[0] = 0xA5;
    digest
}

fn rollback_digest() -> [u8; 32] {
    let mut digest = [0_u8; 32];
    digest[1] = 0xB6;
    digest
}

fn rebuild_digest() -> [u8; 32] {
    let mut digest = [0_u8; 32];
    digest[2] = 0xC7;
    digest
}

fn recovery_digest() -> [u8; 32] {
    let mut digest = [0_u8; 32];
    digest[3] = 0xD8;
    digest
}

#[test]
fn candidate_rejects_missing_catalog_or_stats_version() {
    assert_eq!(
        MapPublicationCandidate::new(descriptor(), 0, 44),
        Err(MapDescriptorError::ZeroCatalogVersion)
    );
    assert_eq!(
        MapPublicationCandidate::new(descriptor(), 33, 0),
        Err(MapDescriptorError::ZeroStatsVersion)
    );
}

#[test]
fn publication_rejects_missing_durable_evidence() {
    assert_eq!(
        MapPublicationEvidence::new(descriptor(), 33, 44, 0, digest()),
        Err(MapDescriptorError::ZeroPublicationLsn)
    );
    assert_eq!(
        MapPublicationEvidence::new(descriptor(), 33, 44, 55, [0_u8; 32]),
        Err(MapDescriptorError::EmptyValidationDigest)
    );
}

#[test]
fn publication_binds_map_to_catalog_stats_and_wal_versions() {
    let candidate = match MapPublicationCandidate::new(descriptor(), 33, 44) {
        Ok(candidate) => candidate,
        Err(error) => panic!("unexpected candidate error: {error}"),
    };
    let evidence = match candidate.publish(55, digest()) {
        Ok(evidence) => evidence,
        Err(error) => panic!("unexpected publication error: {error}"),
    };

    assert!(evidence.is_current_for(33, 44));
    assert!(!evidence.is_current_for(34, 44));
    assert!(!evidence.is_current_for(33, 45));
    assert_eq!(evidence.publication_lsn, 55);
    assert_eq!(evidence.validation_digest, digest());
}

#[test]
fn validated_candidate_is_not_visible_until_active_switch() {
    let candidate = match MapPublicationCandidate::new(descriptor(), 33, 44) {
        Ok(candidate) => candidate,
        Err(error) => panic!("unexpected candidate error: {error}"),
    };
    let validated = match candidate.validate(digest()) {
        Ok(validated) => validated,
        Err(error) => panic!("unexpected validation error: {error}"),
    };

    assert!(!MapPublicationState::Candidate(candidate).is_active_visible());
    assert!(!MapPublicationState::Validated(validated).is_active_visible());

    let switch = match MapPublicationSwitch::from_validated(None, validated, 55) {
        Ok(switch) => switch,
        Err(error) => panic!("unexpected switch error: {error}"),
    };

    assert_eq!(switch.previous_active, None);
    assert_eq!(switch.active.validation_digest, digest());
    assert_eq!(switch.active.publication_lsn, 55);
    assert_eq!(switch.active_state().active(), Some(switch.active));
}

#[test]
fn active_switch_records_previous_active_and_new_active() {
    let previous = match MapPublicationEvidence::new(descriptor(), 33, 44, 55, digest()) {
        Ok(active) => active,
        Err(error) => panic!("unexpected previous active error: {error}"),
    };
    let next_candidate = match MapPublicationCandidate::new(descriptor(), 34, 45) {
        Ok(candidate) => candidate,
        Err(error) => panic!("unexpected next candidate error: {error}"),
    };

    let switch =
        match MapPublicationSwitch::from_candidate(Some(previous), next_candidate, 66, digest()) {
            Ok(switch) => switch,
            Err(error) => panic!("unexpected switch error: {error}"),
        };

    assert_eq!(switch.previous_active, Some(previous));
    assert!(switch.active.is_current_for(34, 45));
    assert_eq!(switch.active.publication_lsn, 66);
    assert_eq!(switch.active_state().active(), Some(switch.active));
}

#[test]
fn rollback_restores_previous_active_without_making_map_truth() {
    let previous = match MapPublicationEvidence::new(descriptor(), 33, 44, 55, digest()) {
        Ok(active) => active,
        Err(error) => panic!("unexpected previous active error: {error}"),
    };
    let replacement = match MapPublicationEvidence::new(descriptor(), 34, 45, 66, digest()) {
        Ok(active) => active,
        Err(error) => panic!("unexpected replacement active error: {error}"),
    };
    let switch = MapPublicationSwitch::new(Some(previous), replacement);

    let rollback = match switch.rollback(77, rollback_digest()) {
        Ok(rollback) => rollback,
        Err(error) => panic!("unexpected rollback error: {error}"),
    };

    assert_eq!(rollback.rolled_back_active, replacement);
    assert_eq!(rollback.active_after_rollback(), Some(previous));
    assert_eq!(rollback.rollback_lsn, 77);
    assert_eq!(
        MapPublicationState::RolledBack(rollback).active(),
        Some(previous)
    );
    assert!(rollback.is_rebuildable_projection());
    assert!(!rollback.is_source_truth());
}

#[test]
fn rollback_rejects_missing_durable_evidence() {
    let active = match MapPublicationEvidence::new(descriptor(), 33, 44, 55, digest()) {
        Ok(active) => active,
        Err(error) => panic!("unexpected active error: {error}"),
    };

    assert_eq!(
        MapPublicationRollbackEvidence::new(active, None, 0, rollback_digest()),
        Err(MapDescriptorError::ZeroRollbackLsn)
    );
    assert_eq!(
        MapPublicationRollbackEvidence::new(active, None, 77, [0_u8; 32]),
        Err(MapDescriptorError::EmptyRollbackDigest)
    );
}

#[test]
fn rebuild_evidence_returns_candidate_for_reconstruction_only() {
    let active = match MapPublicationEvidence::new(descriptor(), 33, 44, 55, digest()) {
        Ok(active) => active,
        Err(error) => panic!("unexpected active error: {error}"),
    };

    let rebuild = match active.rebuild(88, rebuild_digest()) {
        Ok(rebuild) => rebuild,
        Err(error) => panic!("unexpected rebuild error: {error}"),
    };
    let candidate = rebuild.candidate();

    assert_eq!(candidate.descriptor, active.descriptor);
    assert_eq!(candidate.catalog_version, active.catalog_version);
    assert_eq!(candidate.stats_version, active.stats_version);
    assert_eq!(rebuild.rebuild_lsn, 88);
    assert!(!MapPublicationState::RebuildRequired(rebuild).is_active_visible());
    assert!(rebuild.is_rebuildable_projection());
    assert!(!rebuild.is_source_truth());
}

#[test]
fn rebuild_rejects_missing_durable_evidence() {
    let active = match MapPublicationEvidence::new(descriptor(), 33, 44, 55, digest()) {
        Ok(active) => active,
        Err(error) => panic!("unexpected active error: {error}"),
    };

    assert_eq!(
        MapPublicationRebuildEvidence::new(active, 0, rebuild_digest()),
        Err(MapDescriptorError::ZeroRebuildLsn)
    );
    assert_eq!(
        MapPublicationRebuildEvidence::new(active, 88, [0_u8; 32]),
        Err(MapDescriptorError::EmptyRebuildDigest)
    );
}

#[test]
fn recovery_restores_only_durable_active_publication() {
    let active = match MapPublicationEvidence::new(descriptor(), 33, 44, 55, digest()) {
        Ok(active) => active,
        Err(error) => panic!("unexpected active error: {error}"),
    };

    let recovery = match active.recover(99, recovery_digest()) {
        Ok(recovery) => recovery,
        Err(error) => panic!("unexpected recovery error: {error}"),
    };

    assert_eq!(recovery.active_after_recovery(), active);
    assert_eq!(recovery.recovery_lsn, 99);
    assert_eq!(
        MapPublicationState::Recovered(recovery).active(),
        Some(active)
    );
    assert!(recovery.is_rebuildable_projection());
    assert!(!recovery.is_source_truth());
}

#[test]
fn recovery_rejects_missing_durable_evidence() {
    let active = match MapPublicationEvidence::new(descriptor(), 33, 44, 55, digest()) {
        Ok(active) => active,
        Err(error) => panic!("unexpected active error: {error}"),
    };

    assert_eq!(
        MapPublicationRecoveryEvidence::new(active, 0, recovery_digest()),
        Err(MapDescriptorError::ZeroRecoveryLsn)
    );
    assert_eq!(
        MapPublicationRecoveryEvidence::new(active, 99, [0_u8; 32]),
        Err(MapDescriptorError::EmptyRecoveryDigest)
    );
}

#[test]
fn map_publication_is_rebuildable_projection_not_source_truth() {
    let evidence = match MapPublicationEvidence::new(descriptor(), 33, 44, 55, digest()) {
        Ok(evidence) => evidence,
        Err(error) => panic!("unexpected publication error: {error}"),
    };

    assert!(evidence.is_rebuildable_projection());
    assert!(!evidence.is_source_truth());
}
