use super::CatalogChangeNotification;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::sync::{Arc, Mutex};

/// A subscription to catalog changes.
///
/// Clients can poll for changes or await notifications asynchronously.
/// The subscription is automatically invalidated when the server is dropped.
pub trait CatalogChangeSubscription: Send + Sync {
    /// Poll for the next change notification.
    /// Returns `None` if the subscription is closed or no changes are pending.
    fn next_change(&mut self) -> Option<CatalogChangeNotification>;

    /// Check if the subscription is still active.
    fn is_active(&self) -> bool;

    /// Close the subscription explicitly.
    fn close(&mut self);
}

fn catalog_subscription_lock_error(resource: &str) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Catalog,
        format!("catalog subscription {resource} lock is poisoned"),
    )
}

/// In-memory registry for catalog change subscriptions.
///
/// The registry is intentionally small: it records the version-change stream
/// seen by this server runtime and hands each subscriber an isolated cursor.
#[derive(Debug, Clone, Default)]
pub struct CatalogSubscriptionRegistry {
    changes: Arc<Mutex<Vec<CatalogChangeNotification>>>,
}

impl CatalogSubscriptionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&self, change: CatalogChangeNotification) -> AndromedaResult<()> {
        change.validate_version_order()?;
        let mut changes = self
            .changes
            .lock()
            .map_err(|_| catalog_subscription_lock_error("registry"))?;
        if changes.last().is_some_and(|existing| existing == &change) {
            return Ok(());
        }
        if let Some(previous_change) = changes.last()
            && previous_change.new_version != change.previous_version
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog subscription registry must publish changes in catalog version ordering",
            ));
        }
        changes.push(change);
        Ok(())
    }

    pub fn subscribe(&self) -> AndromedaResult<CatalogChangeSubscriptionCursor> {
        let changes = self
            .changes
            .lock()
            .map_err(|_| catalog_subscription_lock_error("registry"))?
            .clone();
        Ok(CatalogChangeSubscriptionCursor::new(changes))
    }
}

/// Cursor implementation of `CatalogChangeSubscription`.
#[derive(Debug, Clone)]
pub struct CatalogChangeSubscriptionCursor {
    changes: Vec<CatalogChangeNotification>,
    index: usize,
    active: bool,
}

impl CatalogChangeSubscriptionCursor {
    pub fn new(changes: Vec<CatalogChangeNotification>) -> Self {
        Self {
            changes,
            index: 0,
            active: true,
        }
    }
}

impl CatalogChangeSubscription for CatalogChangeSubscriptionCursor {
    fn next_change(&mut self) -> Option<CatalogChangeNotification> {
        if !self.active || self.index >= self.changes.len() {
            return None;
        }
        let change = self.changes[self.index];
        self.index += 1;
        Some(change)
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn close(&mut self) {
        self.active = false;
    }
}

/// Mock implementation of `CatalogChangeSubscription` for testing.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct MockCatalogChangeSubscription {
    cursor: CatalogChangeSubscriptionCursor,
}

#[cfg(test)]
impl MockCatalogChangeSubscription {
    pub fn new(changes: Vec<CatalogChangeNotification>) -> Self {
        Self {
            cursor: CatalogChangeSubscriptionCursor::new(changes),
        }
    }
}

#[cfg(test)]
impl CatalogChangeSubscription for MockCatalogChangeSubscription {
    fn next_change(&mut self) -> Option<CatalogChangeNotification> {
        self.cursor.next_change()
    }

    fn is_active(&self) -> bool {
        self.cursor.is_active()
    }

    fn close(&mut self) {
        self.cursor.close();
    }
}
