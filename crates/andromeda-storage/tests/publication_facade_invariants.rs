//! Regression tests pinning publication facade ownership.
//!
//! These tests are the enforcement mechanism for the doctrine documented in
//! `crates/andromeda-storage/src/publication/mod.rs`:
//!
//! 1. `crate::manifest` (re-exported at the crate root) is the **single
//!    canonical owner** of every publication contract type and free function.
//! 2. `crate::publication` is a **pure facade** — every item it exports must
//!    resolve to the exact same type or function as the canonical root path.
//! 3. The facade must never silently shadow a canonical definition with a
//!    duplicate, since duplicating contract types would let cold publication
//!    drift away from the WAL/cold-snapshot truth contract.
//!
//! A failure here means somebody added a real definition inside the
//! `publication` module instead of a `pub use` re-export, or the canonical
//! type was moved without updating the facade. Either case requires going
//! back to `crate::manifest`, restoring single ownership, and re-pointing the
//! facade.

use std::any::TypeId;

use andromeda_storage as canonical;
use andromeda_storage::publication as facade;

#[test]
fn database_manifest_is_facade_alias_of_canonical_owner() {
    assert_eq!(
        TypeId::of::<facade::DatabaseManifest>(),
        TypeId::of::<canonical::DatabaseManifest>(),
        "publication::DatabaseManifest must alias the canonical manifest type",
    );
}

#[test]
fn snapshot_segment_reference_is_facade_alias_of_canonical_owner() {
    assert_eq!(
        TypeId::of::<facade::SnapshotSegmentReference>(),
        TypeId::of::<canonical::SnapshotSegmentReference>(),
        "publication::SnapshotSegmentReference must alias the canonical reference type",
    );
}

#[test]
fn database_snapshot_publication_is_facade_alias_of_canonical_owner() {
    assert_eq!(
        TypeId::of::<facade::DatabaseSnapshotPublication>(),
        TypeId::of::<canonical::DatabaseSnapshotPublication>(),
        "publication::DatabaseSnapshotPublication must alias the canonical publication type",
    );
}

#[test]
fn cold_segment_publication_plan_is_facade_alias_of_canonical_owner() {
    assert_eq!(
        TypeId::of::<facade::ColdSegmentPublicationPlan>(),
        TypeId::of::<canonical::ColdSegmentPublicationPlan>(),
        "publication::ColdSegmentPublicationPlan must alias the canonical cold publication plan",
    );
}

#[test]
fn snapshot_availability_contract_is_facade_alias_of_canonical_owner() {
    assert_eq!(
        TypeId::of::<facade::SnapshotAvailabilityContract>(),
        TypeId::of::<canonical::SnapshotAvailabilityContract>(),
        "publication::SnapshotAvailabilityContract must alias the canonical availability contract",
    );
}

#[test]
fn cold_publication_boundary_validator_is_function_identity_alias() {
    // Function-item types are zero-sized and unique per definition. If the
    // facade re-exported a *wrapper* function the function-pointer coercion
    // would compare unequal even though the signatures match.
    let facade_fn: fn(
        &canonical::SegmentDescriptor,
        &canonical::CoreIoPlacementDecision,
    ) -> andromeda_core::AndromedaResult<()> = facade::validate_cold_segment_publication_boundary;
    let canonical_fn: fn(
        &canonical::SegmentDescriptor,
        &canonical::CoreIoPlacementDecision,
    ) -> andromeda_core::AndromedaResult<()> =
        canonical::validate_cold_segment_publication_boundary;

    assert_eq!(
        facade_fn as usize, canonical_fn as usize,
        "publication::validate_cold_segment_publication_boundary must be the canonical free \
         function, not a facade wrapper that could re-implement the cold publication boundary",
    );
}

fn canonical_manifest() -> canonical::DatabaseManifest {
    canonical::DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: canonical::Lsn::new(10),
        required_wal_start_lsn: canonical::Lsn::new(11),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    }
}

#[test]
fn snapshot_publication_constructed_via_canonical_validates_via_facade() {
    // Build the value through the canonical owner, then route it through the
    // facade alias to prove they refer to the same nominal type and that the
    // facade does not perform any silent transformation of the contract.
    let publication = canonical::DatabaseSnapshotPublication {
        manifest: canonical_manifest(),
        snapshot_id: 3,
        publication_epoch: 1,
        snapshot_descriptor_hash: [4; 32],
        segments: vec![canonical::SnapshotSegmentReference {
            segment_id: canonical::SegmentId::new(5),
            descriptor_hash: [6; 32],
        }],
    };

    let via_facade: facade::DatabaseSnapshotPublication = publication.clone();
    assert_eq!(
        publication, via_facade,
        "facade alias must round-trip the canonical publication value unchanged",
    );

    via_facade
        .validate()
        .expect("facade-aliased publication must satisfy canonical validation");
}

#[test]
fn snapshot_publication_manifest_and_segment_references_are_deterministic() {
    // Cold-snapshot + WAL is the only truth source. The publication contract
    // must therefore be deterministic: two publications built from identical
    // manifest/segment inputs MUST compare equal byte-for-byte (no hidden
    // timestamps, allocator-derived ordering, or pointer-flavoured fields).
    let segments = vec![
        facade::SnapshotSegmentReference {
            segment_id: canonical::SegmentId::new(5),
            descriptor_hash: [6; 32],
        },
        facade::SnapshotSegmentReference {
            segment_id: canonical::SegmentId::new(7),
            descriptor_hash: [8; 32],
        },
    ];

    let first = facade::DatabaseSnapshotPublication {
        manifest: canonical_manifest(),
        snapshot_id: 3,
        publication_epoch: 1,
        snapshot_descriptor_hash: [4; 32],
        segments: segments.clone(),
    };
    let second = facade::DatabaseSnapshotPublication {
        manifest: canonical_manifest(),
        snapshot_id: 3,
        publication_epoch: 1,
        snapshot_descriptor_hash: [4; 32],
        segments,
    };

    assert_eq!(
        first, second,
        "publication contracts must be deterministic across construction",
    );
    first
        .validate()
        .expect("deterministic multi-segment publication must validate");
    assert_eq!(
        first.segments.len(),
        2,
        "segment ordering and count must be preserved verbatim by the facade",
    );
}

#[test]
fn snapshot_availability_contract_alias_enforces_canonical_invariants() {
    // Build through the facade alias and assert that the *canonical* error
    // kind surfaces — proving the facade type is literally the canonical one
    // and not a wrapper that could weaken availability invariants.
    let missing_published = facade::SnapshotAvailabilityContract {
        published_snapshot_id: 3,
        available_snapshot_ids: vec![4],
    };

    let err = missing_published
        .validate()
        .expect_err("published snapshot id must remain in the available set");
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Storage);
}
