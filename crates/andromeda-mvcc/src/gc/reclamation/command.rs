use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use super::{Timestamp, VersionId};

/// Command emitted to GC executor for tuple reclamation.
///
/// The executor uses this to:
/// 1. Locate the tuple in storage
/// 2. Remove it
/// 3. Update version chain pointers
/// 4. Decrement version count
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclamationCommand {
    pub version_id: VersionId,
    pub creator_tx_id: TransactionId,
    pub end_ts: Timestamp,
}

impl ReclamationCommand {
    /// Create a new reclamation command.
    pub fn new(
        version_id: VersionId,
        creator_tx_id: TransactionId,
        end_ts: Timestamp,
    ) -> AndromedaResult<Self> {
        if version_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reclamation command version_id must not be zero",
            ));
        }

        if creator_tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reclamation command creator_tx_id must not be zero",
            ));
        }

        Ok(ReclamationCommand {
            version_id,
            creator_tx_id,
            end_ts,
        })
    }
}
