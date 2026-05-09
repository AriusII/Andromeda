use andromeda_error::AndromedaResult;

use super::catalog_recovery_publication_error;

/// Runtime class for administrative catalog subscribers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSubscriberKind {
    Administration,
    HadrReplica,
}

/// Stable identifier for an administrative/HA catalog subscriber.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogSubscriberId(String);

impl CatalogSubscriberId {
    pub fn new(value: impl Into<String>) -> AndromedaResult<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return catalog_recovery_publication_error("catalog subscriber id must not be empty");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl CatalogSubscriberIdentity for CatalogSubscriberId {
    fn catalog_subscriber_id(&self) -> &str {
        self.as_str()
    }
}

/// Subscriber registration tracked by a catalog publication/subscription runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSubscriberRegistration<TSubscriberId = CatalogSubscriberId> {
    pub subscriber_id: TSubscriberId,
    pub kind: CatalogSubscriberKind,
}

impl<TSubscriberId> CatalogSubscriberRegistration<TSubscriberId> {
    pub fn administration(subscriber_id: TSubscriberId) -> Self {
        Self {
            subscriber_id,
            kind: CatalogSubscriberKind::Administration,
        }
    }

    pub fn hadr_replica(subscriber_id: TSubscriberId) -> Self {
        Self {
            subscriber_id,
            kind: CatalogSubscriberKind::HadrReplica,
        }
    }
}

pub trait CatalogSubscriberIdentity {
    fn catalog_subscriber_id(&self) -> &str;
}

impl CatalogSubscriberIdentity for String {
    fn catalog_subscriber_id(&self) -> &str {
        self.as_str()
    }
}
