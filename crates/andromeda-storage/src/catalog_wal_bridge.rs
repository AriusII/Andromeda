//! Storage adapter exports for catalog WAL byte and publication replay contracts.
//!
//! The deterministic codec and runtime-free replay planner live in
//! `andromeda-catalog-recovery`. Storage keeps these names only as an adapter
//! for concrete storage WAL integration and existing callers.

pub use andromeda_catalog_recovery::{
    CatalogStorageWalDurablePublication as CatalogWalDurablePublication,
    CatalogStorageWalPublicationRecord as CatalogWalPublicationRecord,
    CatalogStorageWalPublicationReplayReport as CatalogWalPublicationReplayReport,
    decode_storage_catalog_record as decode_catalog_record,
    encode_storage_catalog_record as encode_catalog_record,
    replay_storage_catalog_publications_from_wal as replay_catalog_publications_from_wal,
};
