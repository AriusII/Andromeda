use andromeda_maps::{
    MapDescriptor, MapEvidenceAuthority, MapGrain, MapId, MapOwnershipBoundary,
    MapPublicationCandidate, MapPublicationState, MapRefreshMode, MapStalenessPolicy,
};

fn descriptor() -> MapDescriptor {
    MapDescriptor::new(
        MapId::new(41).unwrap(),
        MapGrain::Relation,
        MapRefreshMode::Deferred,
        MapStalenessPolicy::AdvisorySnapshot,
    )
}

#[test]
fn descriptor_and_candidate_are_descriptor_only_boundaries() {
    let descriptor = descriptor();
    let candidate = MapPublicationCandidate::new(descriptor, 3, 5).unwrap();

    assert_eq!(
        descriptor.evidence_authority(),
        MapEvidenceAuthority::DescriptorOnly
    );
    assert_eq!(
        candidate.evidence_authority(),
        MapEvidenceAuthority::DescriptorOnly
    );
    assert!(!descriptor.owns_source_truth());
    assert!(!candidate.owns_source_truth());
    assert!(!candidate.owns_durable_publication_path());
}

#[test]
fn active_publication_state_carries_durable_owner_supplied_evidence_only() {
    let digest = [7; 32];
    let active = MapPublicationCandidate::new(descriptor(), 3, 5)
        .unwrap()
        .validate(digest)
        .unwrap()
        .publish(13)
        .unwrap();
    let state = MapPublicationState::Active(active);

    assert_eq!(
        active.evidence_authority(),
        MapEvidenceAuthority::DurableOwnerSupplied
    );
    assert_eq!(
        state.evidence_authority(),
        MapEvidenceAuthority::DurableOwnerSupplied
    );
    assert!(!active.owns_source_truth());
    assert!(!active.owns_durable_publication_path());
    assert!(!state.owns_source_truth());
    assert!(!state.owns_durable_publication_path());
}
