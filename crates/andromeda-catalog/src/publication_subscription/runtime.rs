use crate::{CatalogDurabilityMarker, CatalogPublicationReceipt};

use super::CatalogSubscriberId;

pub type CatalogPublicationRuntimeState =
    andromeda_catalog_recovery::CatalogPublicationRuntimeState<CatalogDurabilityMarker>;

pub type CatalogSubscriberAckProgress = andromeda_catalog_recovery::CatalogSubscriberAckProgress<
    CatalogSubscriberId,
    CatalogDurabilityMarker,
>;

pub type CatalogHadrSubscriberReplayEvidence =
    andromeda_catalog_recovery::CatalogHadrSubscriberReplayEvidence<
        CatalogSubscriberId,
        CatalogDurabilityMarker,
    >;

pub type CatalogPublicationSubscriberRegistry =
    andromeda_catalog_recovery::CatalogPublicationSubscriberRegistry<
        CatalogPublicationReceipt,
        CatalogSubscriberId,
    >;
