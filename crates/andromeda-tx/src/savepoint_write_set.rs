//! Storage-agnostic transaction write-set support for savepoint rollback.
//!
//! The write-set records transaction-local writes with monotonically
//! increasing ordinals. Savepoint rollback drains only entries whose ordinal is
//! newer than the savepoint marker; lock ownership and durable storage remain
//! outside this module.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::savepoint::{SavepointId, SavepointRollbackMarker};

/// Maximum encoded bytes accepted for a resource identifier.
pub const MAX_WRITE_SET_RESOURCE_ID_BYTES: usize = 1024;

/// Maximum encoded bytes accepted for before/after images.
pub const MAX_WRITE_SET_IMAGE_BYTES: usize = 64 * 1024;

/// Transaction-local write ordinal.
pub type WriteSetOrdinal = u64;

/// Coarse, storage-agnostic operation classification for a write-set entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WriteSetOperationKind {
    Insert,
    Update,
    Delete,
    /// Placeholder for evidence that must survive partial savepoint rollback,
    /// such as a lock/acquire trace owned by the outer transaction.
    LockEvidence,
    Custom(String),
}

/// Stable, bounded resource identity for write-set entries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WriteSetResourceId {
    Text(String),
    Bytes(Vec<u8>),
}

impl WriteSetResourceId {
    pub fn try_from_text(value: impl Into<String>) -> AndromedaResult<Self> {
        let value = value.into();
        validate_non_empty_bounded(
            value.as_bytes(),
            MAX_WRITE_SET_RESOURCE_ID_BYTES,
            "write-set resource id must not be empty",
            "write-set resource id exceeds maximum size",
        )?;
        Ok(Self::Text(value))
    }

    pub fn try_from_bytes(value: impl Into<Vec<u8>>) -> AndromedaResult<Self> {
        let value = value.into();
        validate_non_empty_bounded(
            &value,
            MAX_WRITE_SET_RESOURCE_ID_BYTES,
            "write-set resource id must not be empty",
            "write-set resource id exceeds maximum size",
        )?;
        Ok(Self::Bytes(value))
    }

    pub fn encoded_len(&self) -> usize {
        match self {
            Self::Text(value) => value.len(),
            Self::Bytes(value) => value.len(),
        }
    }
}

/// Bounded opaque before/after image.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WriteSetImage {
    bytes: Vec<u8>,
}

impl WriteSetImage {
    pub fn try_from_bytes(value: impl Into<Vec<u8>>) -> AndromedaResult<Self> {
        let bytes = value.into();
        if bytes.len() > MAX_WRITE_SET_IMAGE_BYTES {
            return Err(transaction_error("write-set image exceeds maximum size"));
        }
        Ok(Self { bytes })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// A single transaction-local write recorded for commit or partial rollback.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WriteSetEntry {
    pub ordinal: WriteSetOrdinal,
    pub operation_kind: WriteSetOperationKind,
    pub resource_id: WriteSetResourceId,
    pub before_image: Option<WriteSetImage>,
    pub after_image: Option<WriteSetImage>,
}

/// Minimal in-memory write-set used to connect savepoint markers to writes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TxWriteSet {
    next_ordinal: WriteSetOrdinal,
    entries: Vec<WriteSetEntry>,
}

impl TxWriteSet {
    pub const fn new() -> Self {
        Self {
            next_ordinal: 1,
            entries: Vec::new(),
        }
    }

    pub fn record_operation(
        &mut self,
        operation_kind: WriteSetOperationKind,
        resource_id: WriteSetResourceId,
        before_image: Option<WriteSetImage>,
        after_image: Option<WriteSetImage>,
    ) -> AndromedaResult<WriteSetEntry> {
        let ordinal = self.next_ordinal;
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or_else(|| transaction_error("write-set ordinal allocator overflowed"))?;

        let entry = WriteSetEntry {
            ordinal,
            operation_kind,
            resource_id,
            before_image,
            after_image,
        };
        self.entries.push(entry.clone());
        Ok(entry)
    }

    /// Current rollback ordinal for a savepoint created after all recorded
    /// writes currently in the set.
    pub fn rollback_ordinal(&self) -> WriteSetOrdinal {
        self.entries.last().map_or(0, |entry| entry.ordinal)
    }

    /// Build a savepoint rollback marker from the current write-set ordinal.
    pub fn savepoint_marker(
        &self,
        savepoint_id: SavepointId,
    ) -> AndromedaResult<SavepointRollbackMarker> {
        SavepointRollbackMarker::new(savepoint_id, self.rollback_ordinal())
    }

    /// Remove and return undoable entries recorded after `marker`.
    ///
    /// Returned entries are ordered from newest to oldest so callers can apply
    /// undo in reverse write order. Lock evidence belongs to the outer
    /// transaction lock lifecycle and survives partial savepoint rollback.
    pub fn rollback_to(
        &mut self,
        marker: SavepointRollbackMarker,
    ) -> AndromedaResult<Vec<WriteSetEntry>> {
        marker.validate()?;
        if marker.rollback_ordinal > self.rollback_ordinal() {
            return Err(transaction_error(
                "savepoint rollback marker is ahead of the write-set",
            ));
        }

        let rollback_index = match self
            .entries
            .iter()
            .position(|entry| entry.ordinal > marker.rollback_ordinal)
        {
            Some(index) => index,
            None => self.entries.len(),
        };
        let drained = self.entries.split_off(rollback_index);
        let mut undo_entries = Vec::with_capacity(drained.len());

        for entry in drained {
            if entry.operation_kind == WriteSetOperationKind::LockEvidence {
                self.entries.push(entry);
            } else {
                undo_entries.push(entry);
            }
        }

        undo_entries.reverse();
        Ok(undo_entries)
    }

    /// Releasing a savepoint does not alter the write-set.
    pub fn release(&mut self, marker: SavepointRollbackMarker) -> AndromedaResult<()> {
        marker.validate()
    }

    /// Drain all writes during full transaction rollback.
    ///
    /// Entries are returned from newest to oldest so callers can undo writes
    /// before releasing transaction-level evidence such as locks.
    pub fn full_rollback(&mut self) -> Vec<WriteSetEntry> {
        let mut drained = self.entries.drain(..).collect::<Vec<_>>();
        drained.reverse();
        drained
    }

    pub fn entries(&self) -> &[WriteSetEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn validate_non_empty_bounded(
    bytes: &[u8],
    max_len: usize,
    empty_message: &'static str,
    oversize_message: &'static str,
) -> AndromedaResult<()> {
    if bytes.is_empty() {
        return Err(transaction_error(empty_message));
    }
    if bytes.len() > max_len {
        return Err(transaction_error(oversize_message));
    }
    Ok(())
}

fn transaction_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Transaction, message)
}
