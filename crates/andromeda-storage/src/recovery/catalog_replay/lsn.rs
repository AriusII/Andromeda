use crate::wal_record_catalog::CatalogWalRecord;

use super::snapshot::CatalogSnapshot;

pub type LsnBoundCatalogRecord =
    andromeda_catalog_recovery::LsnBoundCatalogRecord<CatalogWalRecord>;

pub type CatalogReplayFromLsnReport =
    andromeda_catalog_recovery::CatalogReplayFromLsnReport<CatalogSnapshot>;
