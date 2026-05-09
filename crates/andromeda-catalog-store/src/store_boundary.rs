//! Runtime-free catalog store boundary DTOs.
//!
//! These types describe store-facing report and WAL append shapes without
//! owning snapshots, DefinitionBatch planning, WAL persistence, or publication.

/// Store-facing report for applying a planned catalog mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogStoreApplyReport<Plan, SnapshotReport, SourceHash, DependencyGraphHash> {
    pub plan: Plan,
    pub snapshot_report: SnapshotReport,
    pub source_hash: SourceHash,
    pub dependency_graph_hash: DependencyGraphHash,
}

/// WAL append evidence for one catalog mutation record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogStoreWalAppend<RecordKind> {
    pub kind: RecordKind,
    pub lsn: u64,
}

/// Store-facing report for a definition batch that was appended, flushed, and
/// then published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogStoreDurableApplyReport<
    Plan,
    Receipt,
    RecordKind,
    SourceHash,
    DependencyGraphHash,
> {
    pub plan: Plan,
    pub receipt: Receipt,
    pub appended_records: Vec<CatalogStoreWalAppend<RecordKind>>,
    pub source_hash: SourceHash,
    pub dependency_graph_hash: DependencyGraphHash,
}

/// Minimal behavior required to validate a store append sequence.
pub trait CatalogStoreMutationKind: Copy + Eq {
    fn is_commit_record(self) -> bool;
}

/// Validation error for a catalog store append sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogStoreWalAppendSequenceError {
    MissingPlannedRecord,
    MissingCommitRecord,
    KindMismatch { index: usize },
    ZeroLsn { index: usize },
    NonIncreasingLsn { index: usize },
}

impl CatalogStoreWalAppendSequenceError {
    pub const fn message(self) -> &'static str {
        match self {
            Self::MissingPlannedRecord => {
                "catalog WAL append sequence must include every planned mutation record"
            },
            Self::MissingCommitRecord => {
                "catalog WAL append sequence must end with the commit record"
            },
            Self::KindMismatch { .. } => "catalog WAL append kind must match the mutation plan",
            Self::ZeroLsn { .. } => "catalog WAL append LSN must not be zero",
            Self::NonIncreasingLsn { .. } => "catalog WAL append LSNs must be strictly increasing",
        }
    }
}

/// Validates that appended WAL records match the planned catalog mutation
/// sequence and reach a terminal commit record.
pub fn validate_catalog_store_wal_append_sequence<RecordKind>(
    appended_records: &[CatalogStoreWalAppend<RecordKind>],
    expected_kinds: &[RecordKind],
) -> Result<(), CatalogStoreWalAppendSequenceError>
where
    RecordKind: CatalogStoreMutationKind,
{
    if appended_records.len() != expected_kinds.len() {
        return Err(CatalogStoreWalAppendSequenceError::MissingPlannedRecord);
    }

    if !appended_records
        .last()
        .is_some_and(|record| record.kind.is_commit_record())
    {
        return Err(CatalogStoreWalAppendSequenceError::MissingCommitRecord);
    }

    let mut previous_lsn = None;
    for (index, (append, expected_kind)) in appended_records
        .iter()
        .zip(expected_kinds.iter())
        .enumerate()
    {
        if append.kind != *expected_kind {
            return Err(CatalogStoreWalAppendSequenceError::KindMismatch { index });
        }

        if append.lsn == 0 {
            return Err(CatalogStoreWalAppendSequenceError::ZeroLsn { index });
        }

        if previous_lsn.is_some_and(|previous_lsn| append.lsn <= previous_lsn) {
            return Err(CatalogStoreWalAppendSequenceError::NonIncreasingLsn { index });
        }

        previous_lsn = Some(append.lsn);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestRecordKind {
        Begin,
        Commit,
    }

    impl CatalogStoreMutationKind for TestRecordKind {
        fn is_commit_record(self) -> bool {
            self == Self::Commit
        }
    }

    #[test]
    fn append_sequence_requires_commit_terminal_record() {
        let appended = [CatalogStoreWalAppend {
            kind: TestRecordKind::Begin,
            lsn: 10,
        }];
        let expected = [TestRecordKind::Begin];

        assert_eq!(
            validate_catalog_store_wal_append_sequence(&appended, &expected),
            Err(CatalogStoreWalAppendSequenceError::MissingCommitRecord)
        );
    }

    #[test]
    fn append_sequence_rejects_non_increasing_lsn() {
        let appended = [
            CatalogStoreWalAppend {
                kind: TestRecordKind::Begin,
                lsn: 10,
            },
            CatalogStoreWalAppend {
                kind: TestRecordKind::Commit,
                lsn: 10,
            },
        ];
        let expected = [TestRecordKind::Begin, TestRecordKind::Commit];

        assert_eq!(
            validate_catalog_store_wal_append_sequence(&appended, &expected),
            Err(CatalogStoreWalAppendSequenceError::NonIncreasingLsn { index: 1 })
        );
    }
}
