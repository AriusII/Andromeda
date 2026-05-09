//! Snapshot publication state bound to catalog mutation receipts.

use crate::CatalogPublicationReceipt;

pub type CatalogSnapshotPublication =
    andromeda_catalog_store::CatalogSnapshotPublication<CatalogPublicationReceipt>;
