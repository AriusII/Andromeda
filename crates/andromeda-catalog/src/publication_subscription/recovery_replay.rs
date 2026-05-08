pub use andromeda_catalog_recovery::{
    CatalogPublicationReplayKey, CatalogPublicationReplayTerminalOutcome,
    CatalogPublicationSubscriptionReplayEvidence, CatalogPublicationSubscriptionReplayRecordKind,
    CatalogRecoveryReplayExpectation, CatalogSubscriptionReplayKey,
};
use andromeda_error::AndromedaResult;

use super::CatalogSubscriberId;
use crate::{CatalogDurabilityMarker, CatalogPublicationReceipt};

pub type CatalogPublicationReplayTerminalRecord =
    andromeda_catalog_recovery::CatalogPublicationReplayTerminalRecord<CatalogDurabilityMarker>;

pub type CatalogPublicationSubscriptionReplayRecord =
    andromeda_catalog_recovery::CatalogPublicationSubscriptionReplayRecord<
        CatalogPublicationReceipt,
        CatalogSubscriberId,
    >;

pub type CatalogPublicationSubscriptionReplaySummary =
    andromeda_catalog_recovery::CatalogPublicationSubscriptionReplaySummary;

pub fn replay_publication_subscription_changes(
    records: impl IntoIterator<Item = CatalogPublicationSubscriptionReplayRecord>,
) -> AndromedaResult<CatalogPublicationSubscriptionReplaySummary> {
    andromeda_catalog_recovery::replay_publication_subscription_changes(records)
}
