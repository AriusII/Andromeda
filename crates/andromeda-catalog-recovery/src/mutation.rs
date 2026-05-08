/// Durable catalog mutation payload plus optional outer storage-WAL kind tag.
///
/// When `storage_wal_kind_tag` is present, the catalog owner must enforce that
/// the outer storage WAL record kind matches the inner catalog payload kind
/// before the record can participate in replay.
#[derive(Debug, Clone, Copy)]
pub struct CatalogDurableMutationPayload<'a> {
    pub payload: &'a [u8],
    pub storage_wal_kind_tag: Option<u16>,
}

impl<'a> CatalogDurableMutationPayload<'a> {
    pub const fn new(payload: &'a [u8]) -> Self {
        Self {
            payload,
            storage_wal_kind_tag: None,
        }
    }

    pub const fn with_storage_wal_kind_tag(payload: &'a [u8], storage_wal_kind_tag: u16) -> Self {
        Self {
            payload,
            storage_wal_kind_tag: Some(storage_wal_kind_tag),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSkippedBatchReason {
    BeginSupersededByAnotherBegin,
    EndOfLogBeforeCommit,
    CommitBoundaryMismatch,
    MissingApplyRecords,
    DuplicateApplyIndex,
    SparseApplyIndexes,
    ApplyRecordCountMismatch,
    ApplyRecordLimitExceeded,
    ApplyRecordOrderMismatch,
    WrongCatalogIdentity,
    VersionGap,
    DefinitionBatchHashMismatch,
    PlanRejected,
    ReplayRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogRecoveryAnomalyKind {
    PayloadCorruption,
    PayloadMagicMismatch,
    PayloadFormatVersionMismatch,
    PayloadChecksumMismatch,
    WrongKindTag,
    OuterStorageKindMismatch,
    CommitWithoutBegin,
    ApplyWithoutBegin,
    BeginWhileBatchOpen,
    CommitBoundaryMismatch,
    WrongCatalogIdentity,
    VersionGap,
    MissingApplyRecords,
    DuplicateApplyIndex,
    SparseApplyIndexes,
    ApplyRecordCountMismatch,
    ApplyRecordLimitExceeded,
    ApplyRecordOrderMismatch,
    PlanRejected,
    DefinitionBatchHashMismatch,
    ReplayRejected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_payload_preserves_optional_storage_kind_tag() {
        let payload = [1, 2, 3];

        let untagged = CatalogDurableMutationPayload::new(&payload);
        assert_eq!(untagged.payload, payload);
        assert_eq!(untagged.storage_wal_kind_tag, None);

        let tagged = CatalogDurableMutationPayload::with_storage_wal_kind_tag(&payload, 42);
        assert_eq!(tagged.payload, payload);
        assert_eq!(tagged.storage_wal_kind_tag, Some(42));
    }
}
