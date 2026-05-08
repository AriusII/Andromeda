use crate::CatalogPublicationReceipt;

/// Durable publication report consumed only by administrative/HA subscribers.
pub type CatalogPublicationReport =
    andromeda_catalog_recovery::CatalogPublicationReport<CatalogPublicationReceipt>;
