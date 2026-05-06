use super::CatalogChangeNotification;

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

/// Mock implementation of `CatalogChangeSubscription` for testing.
#[derive(Debug, Clone)]
pub struct MockCatalogChangeSubscription {
    changes: Vec<CatalogChangeNotification>,
    index: usize,
    active: bool,
}

impl MockCatalogChangeSubscription {
    pub fn new(changes: Vec<CatalogChangeNotification>) -> Self {
        Self {
            changes,
            index: 0,
            active: true,
        }
    }
}

impl CatalogChangeSubscription for MockCatalogChangeSubscription {
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
