//! Transaction savepoint state contract.
//!
//! Savepoints are intra-transaction rollback markers. They do not publish MVCC
//! visibility, do not make commit/rollback durable, and do not release locks.
//! Storage engines use the returned evidence to undo local, not-yet-visible
//! writes while the transaction remains `Active` and `InFlight`.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Monotonic, transaction-local savepoint identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SavepointId(u64);

impl SavepointId {
    pub const MIN_VALID: u64 = 1;

    /// Build a savepoint id without validation.
    ///
    /// This remains available for compatibility with existing callers that
    /// round-trip persisted ids. New savepoint ids should be allocated through
    /// `SavepointStack` or checked with `try_new`.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn try_new(value: u64) -> AndromedaResult<Self> {
        let id = Self::new(value);
        id.validate()?;
        Ok(id)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub const fn is_valid(self) -> bool {
        !self.is_zero() && self.0 >= Self::MIN_VALID
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if !self.is_valid() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "savepoint id must be non-zero",
            ));
        }

        Ok(())
    }
}

/// Stable marker used by a storage/MVCC write set to undo to a savepoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavepointRollbackMarker {
    pub savepoint_id: SavepointId,
    /// Transaction-local ordering value. Writers that registered after this
    /// ordinal belong to this savepoint's descendants or later work and must be
    /// undone by rollback-to before the transaction can continue.
    pub rollback_ordinal: u64,
}

/// Savepoint entry retained on the active transaction stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Savepoint {
    pub id: SavepointId,
    pub name: String,
    pub rollback_marker: SavepointRollbackMarker,
}

/// Evidence returned after `ROLLBACK TO SAVEPOINT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavepointRollbackEvidence {
    pub target: Savepoint,
    /// Savepoints removed because they were nested inside the target.
    pub discarded_descendants: Vec<Savepoint>,
}

/// Evidence returned after `RELEASE SAVEPOINT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavepointReleaseEvidence {
    /// Released savepoint plus any nested descendants, in stack order.
    pub released: Vec<Savepoint>,
}

/// Transaction-local savepoint stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavepointStack {
    next_id: u64,
    stack: Vec<Savepoint>,
}

impl SavepointStack {
    pub const fn new() -> Self {
        Self {
            next_id: 1,
            stack: Vec::new(),
        }
    }

    pub fn create(&mut self, name: impl Into<String>) -> AndromedaResult<Savepoint> {
        let name = name.into();
        validate_name(&name)?;
        if self.stack.iter().any(|savepoint| savepoint.name == name) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "savepoint name must be unique within a transaction",
            ));
        }

        let id = SavepointId::try_new(self.next_id)?;
        self.next_id = self.next_id.checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "savepoint id allocator overflowed",
            )
        })?;

        let savepoint = Savepoint {
            id,
            name,
            rollback_marker: SavepointRollbackMarker {
                savepoint_id: id,
                rollback_ordinal: id.get(),
            },
        };
        self.stack.push(savepoint.clone());
        Ok(savepoint)
    }

    /// Release the named savepoint and all savepoints nested after it.
    pub fn release(&mut self, name: &str) -> AndromedaResult<SavepointReleaseEvidence> {
        validate_name(name)?;
        let index = self.find_index(name)?;
        let released = self.stack.split_off(index);
        Ok(SavepointReleaseEvidence { released })
    }

    /// Roll back to the named savepoint, discarding only nested descendants.
    ///
    /// The target savepoint remains active, matching the standard savepoint
    /// rollback shape and allowing repeated rollback-to of the same marker.
    pub fn rollback_to(&mut self, name: &str) -> AndromedaResult<SavepointRollbackEvidence> {
        validate_name(name)?;
        let index = self.find_index(name)?;
        let target = self.stack[index].clone();
        let discarded_descendants = self.stack.split_off(index + 1);
        Ok(SavepointRollbackEvidence {
            target,
            discarded_descendants,
        })
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn active(&self) -> &[Savepoint] {
        &self.stack
    }

    fn find_index(&self, name: &str) -> AndromedaResult<usize> {
        self.stack
            .iter()
            .position(|savepoint| savepoint.name == name)
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "savepoint is not active in this transaction",
                )
            })
    }
}

impl Default for SavepointStack {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_name(name: &str) -> AndromedaResult<()> {
    if name.trim().is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "savepoint name must not be empty",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_requires_unique_non_empty_names_and_monotonic_ids() {
        let mut stack = SavepointStack::new();
        assert!(stack.create("").is_err());

        let a = stack.create("a").unwrap();
        let b = stack.create("b").unwrap();
        assert_eq!(a.id.get(), 1);
        assert_eq!(b.id.get(), 2);
        assert_eq!(stack.depth(), 2);
        assert!(stack.create("a").is_err());
    }

    #[test]
    fn rollback_to_discards_descendants_and_keeps_target_active() {
        let mut stack = SavepointStack::new();
        stack.create("a").unwrap();
        stack.create("b").unwrap();
        stack.create("c").unwrap();

        let evidence = stack.rollback_to("b").unwrap();
        assert_eq!(evidence.target.name.as_str(), "b");
        assert_eq!(evidence.discarded_descendants.len(), 1);
        assert_eq!(evidence.discarded_descendants[0].name.as_str(), "c");
        assert_eq!(
            stack
                .active()
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn release_discards_target_and_descendants() {
        let mut stack = SavepointStack::new();
        stack.create("a").unwrap();
        stack.create("b").unwrap();
        stack.create("c").unwrap();

        let evidence = stack.release("b").unwrap();
        assert_eq!(
            evidence
                .released
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c"]
        );
        assert_eq!(
            stack
                .active()
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a"]
        );
    }

    #[test]
    fn try_new_rejects_zero_id() {
        let err = SavepointId::try_new(0).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    }
}
