use andromeda_core::AndromedaResult;

use super::catalog_publication_error;

/// Stable identifier for an administrative/HA catalog subscriber.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogSubscriberId(String);

impl CatalogSubscriberId {
    pub fn new(value: impl Into<String>) -> AndromedaResult<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return catalog_publication_error("catalog subscriber id must not be empty");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Runtime class for administrative catalog subscribers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSubscriberKind {
    Administration,
    HadrReplica,
}

/// Subscriber registration tracked by the publication/subscription runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSubscriberRegistration {
    pub subscriber_id: CatalogSubscriberId,
    pub kind: CatalogSubscriberKind,
}

impl CatalogSubscriberRegistration {
    pub fn administration(subscriber_id: CatalogSubscriberId) -> Self {
        Self {
            subscriber_id,
            kind: CatalogSubscriberKind::Administration,
        }
    }

    pub fn hadr_replica(subscriber_id: CatalogSubscriberId) -> Self {
        Self {
            subscriber_id,
            kind: CatalogSubscriberKind::HadrReplica,
        }
    }
}
