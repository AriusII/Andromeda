use crate::{Lsn, WalRecordKind};

/// Handler result for a single WAL record replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayOutcome {
    /// Record was successfully applied to the database state.
    Applied,
    /// Record was skipped in this recovery context.
    Skipped,
    /// Record handler is not promoted; recovery must fail-stop if records of
    /// this type require redo.
    NotYetImplemented,
    /// Record is deprecated and should not appear in new WAL files.
    Deprecated,
}

impl ReplayOutcome {
    pub const fn is_applied(self) -> bool {
        matches!(self, Self::Applied)
    }

    pub const fn is_error(self) -> bool {
        matches!(self, Self::NotYetImplemented | Self::Deprecated)
    }
}

/// Result of replaying a single WAL record with metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayResult {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub outcome: ReplayOutcome,
    pub error: Option<String>,
}

impl ReplayResult {
    pub fn applied(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::Applied,
            error: None,
        }
    }

    pub fn skipped(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::Skipped,
            error: None,
        }
    }

    pub fn deferred(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::NotYetImplemented,
            error: Some(format!(
                "{:?} recovery handler is not promoted; redo payload decoding and idempotent apply semantics must be implemented before records of this type can replay.",
                kind
            )),
        }
    }

    pub fn deprecated(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::Deprecated,
            error: Some(format!(
                "{:?} is deprecated and should not appear in new WAL files.",
                kind
            )),
        }
    }

    pub fn error(lsn: Lsn, kind: WalRecordKind, error_msg: impl Into<String>) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::NotYetImplemented,
            error: Some(error_msg.into()),
        }
    }
}
