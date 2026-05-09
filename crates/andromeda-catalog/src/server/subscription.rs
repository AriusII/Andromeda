//! Compatibility reexports for catalog change subscription primitives.

#[cfg(test)]
use andromeda_catalog_store::CatalogChangeNotification;
pub use andromeda_catalog_store::{
    CatalogChangeSubscription, CatalogChangeSubscriptionCursor, CatalogSubscriptionRegistry,
};

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
