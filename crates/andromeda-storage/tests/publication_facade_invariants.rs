//! Regression guard for the remaining `andromeda_storage::publication`
//! compatibility surface.
//!
//! The storage publication facade was pruned to the only active compatibility
//! import path: `publication::DatabaseManifest`.

use std::any::TypeId;

use andromeda_manifest as manifest_owner;
use andromeda_storage::publication as facade;

#[test]
fn database_manifest_is_facade_alias_of_owner_manifest() {
    assert_eq!(
        TypeId::of::<facade::DatabaseManifest>(),
        TypeId::of::<manifest_owner::DatabaseManifest>(),
        "publication::DatabaseManifest must alias andromeda_manifest::DatabaseManifest",
    );
}

#[test]
fn database_manifest_constructed_via_owner_validates_via_facade() {
    let manifest = manifest_owner::DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: andromeda_wal::Lsn::new(10),
        required_wal_start_lsn: andromeda_wal::Lsn::new(11),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };

    let via_facade: facade::DatabaseManifest = manifest;
    via_facade
        .validate()
        .expect("facade manifest must use owner validation");
}
