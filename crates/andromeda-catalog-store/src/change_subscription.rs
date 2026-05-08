//! Catalog change subscription primitives.
//!
//! The registry is an in-memory cursor source for runtime consumers. Durable
//! publication and recovery remain outside this crate.

use std::sync::{Arc, Mutex};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ProcedureId};

/// A change notification for catalog updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogChangeNotification {
    /// The new catalog version after the change.
    pub new_version: CatalogVersion,
    /// The previous catalog version.
    pub previous_version: CatalogVersion,
    /// Log sequence number boundary for LSN-aware invalidation.
    pub invalidation_boundary_lsn: u64,
}

impl CatalogChangeNotification {
    pub fn validate_version_order(&self) -> AndromedaResult<()> {
        if self.previous_version.get() == 0 || self.new_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog change notification versions must not be zero",
            ));
        }
        if self.new_version <= self.previous_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog change notification must advance catalog version ordering",
            ));
        }
        if self.invalidation_boundary_lsn == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog change notification invalidation boundary LSN must not be zero",
            ));
        }
        let expected_next = self.previous_version.get().checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog change notification version ordering overflowed",
            )
        })?;
        if self.new_version.get() != expected_next {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog change notification version ordering must advance by one catalog version",
            ));
        }
        Ok(())
    }

    /// Determine if the change affected a specific procedure.
    pub fn affects_procedure(&self, _procedure_id: ProcedureId) -> bool {
        self.new_version != self.previous_version
    }
}

/// A subscription to catalog changes.
///
/// Clients can poll for changes or await notifications asynchronously. The
/// subscription is automatically invalidated when the server is dropped.
pub trait CatalogChangeSubscription: Send + Sync {
    /// Poll for the next change notification.
    ///
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

#[cfg(test)]
mod tests {
    use super::*;

    fn change(
        previous_version: u64,
        new_version: u64,
        invalidation_boundary_lsn: u64,
    ) -> CatalogChangeNotification {
        CatalogChangeNotification {
            previous_version: CatalogVersion::new(previous_version),
            new_version: CatalogVersion::new(new_version),
            invalidation_boundary_lsn,
        }
    }

    #[test]
    fn registry_requires_ordered_version_changes() {
        let registry = CatalogSubscriptionRegistry::new();
        let first = change(1, 2, 100);

        registry.publish(first).unwrap();
        registry.publish(first).unwrap();

        let mut subscription = registry.subscribe().unwrap();
        assert_eq!(subscription.next_change(), Some(first));
        assert!(subscription.next_change().is_none());

        let out_of_order = change(2, 4, 200);
        let error = registry.publish(out_of_order).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("version ordering"));
    }

    #[test]
    fn notification_requires_non_zero_lsn() {
        let error = change(1, 2, 0).validate_version_order().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("boundary LSN"));
    }

    #[test]
    fn cursor_can_be_closed() {
        let mut cursor = CatalogChangeSubscriptionCursor::new(vec![change(1, 2, 100)]);

        assert!(cursor.is_active());
        assert_eq!(cursor.next_change(), Some(change(1, 2, 100)));
        assert!(cursor.next_change().is_none());

        cursor.close();
        assert!(!cursor.is_active());
    }
}
