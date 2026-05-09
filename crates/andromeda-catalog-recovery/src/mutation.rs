use andromeda_catalog_store::CatalogStoreMutationKind;
pub use andromeda_catalog_store::{
    CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH, CatalogPublicationSemantics,
};
pub use andromeda_definition_batch::{CatalogLifecycleTarget, DefinitionBatchDependencyGraphHash};
use andromeda_definition_batch::{DefinitionBatchId, DefinitionBatchSourceHash};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

pub const CATALOG_CHANGE_BEGIN_WAL_KIND_TAG: u16 = 19;
pub const CATALOG_CHANGE_APPLY_WAL_KIND_TAG: u16 = 20;
pub const CATALOG_CHANGE_COMMIT_WAL_KIND_TAG: u16 = 21;

pub type CatalogMutationBoundary = andromeda_catalog_store::CatalogMutationBoundary<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>;
pub type CatalogMutationDelta =
    andromeda_catalog_store::CatalogMutationDelta<CatalogLifecycleTarget>;
pub type CatalogMutationOperation =
    andromeda_catalog_store::CatalogMutationOperation<CatalogLifecycleTarget>;

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
pub enum CatalogMutationRecordKind {
    CatalogChangeBegin,
    CatalogChangeApply,
    CatalogChangeCommit,
}

impl CatalogMutationRecordKind {
    pub const fn storage_wal_kind_tag(self) -> u16 {
        match self {
            Self::CatalogChangeBegin => CATALOG_CHANGE_BEGIN_WAL_KIND_TAG,
            Self::CatalogChangeApply => CATALOG_CHANGE_APPLY_WAL_KIND_TAG,
            Self::CatalogChangeCommit => CATALOG_CHANGE_COMMIT_WAL_KIND_TAG,
        }
    }

    pub const fn from_storage_wal_kind_tag(tag: u16) -> Option<Self> {
        match tag {
            CATALOG_CHANGE_BEGIN_WAL_KIND_TAG => Some(Self::CatalogChangeBegin),
            CATALOG_CHANGE_APPLY_WAL_KIND_TAG => Some(Self::CatalogChangeApply),
            CATALOG_CHANGE_COMMIT_WAL_KIND_TAG => Some(Self::CatalogChangeCommit),
            _ => None,
        }
    }
}

impl CatalogStoreMutationKind for CatalogMutationRecordKind {
    fn is_commit_record(self) -> bool {
        self == Self::CatalogChangeCommit
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogWalPayloadDecodeErrorKind {
    TruncatedHeader,
    MagicMismatch,
    LegacyFormatVersion,
    UnsupportedFormatVersion,
    UnknownRecordKindTag,
    BodyLengthOverflow,
    BodyLengthMismatch,
    ChecksumMismatch,
    BodyInvalid,
}

impl CatalogWalPayloadDecodeErrorKind {
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::TruncatedHeader => "catalog_wal_payload_truncated_header",
            Self::MagicMismatch => "catalog_wal_payload_magic_mismatch",
            Self::LegacyFormatVersion => "catalog_wal_payload_legacy_format_version",
            Self::UnsupportedFormatVersion => "catalog_wal_payload_unsupported_format_version",
            Self::UnknownRecordKindTag => "catalog_wal_payload_unknown_record_kind_tag",
            Self::BodyLengthOverflow => "catalog_wal_payload_body_length_overflow",
            Self::BodyLengthMismatch => "catalog_wal_payload_body_length_mismatch",
            Self::ChecksumMismatch => "catalog_wal_payload_checksum_mismatch",
            Self::BodyInvalid => "catalog_wal_payload_body_invalid",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogWalPayloadDecodeError {
    kind: CatalogWalPayloadDecodeErrorKind,
    detail: String,
}

impl CatalogWalPayloadDecodeError {
    pub fn new(kind: CatalogWalPayloadDecodeErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub(crate) fn from_body_error(error: AndromedaError) -> Self {
        Self::new(
            CatalogWalPayloadDecodeErrorKind::BodyInvalid,
            error.message().to_string(),
        )
    }

    pub const fn kind(&self) -> CatalogWalPayloadDecodeErrorKind {
        self.kind
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl From<CatalogWalPayloadDecodeError> for AndromedaError {
    fn from(error: CatalogWalPayloadDecodeError) -> Self {
        AndromedaError::new(
            AndromedaErrorKind::Catalog,
            format!("{}: {}", error.kind.stable_code(), error.detail),
        )
    }
}

/// Runtime-free recovery boundary for applying a verified committed catalog
/// mutation to a live catalog owner.
///
/// `andromeda-catalog-recovery` owns the WAL/replay DTOs, but it deliberately
/// does not own `CatalogSnapshot` or any concrete runtime state. Catalog owners
/// implement this trait locally to adapt committed recovery batches without
/// moving snapshot storage into this crate.
pub trait CatalogRecoveryApplyTarget {
    fn recovery_database_id(&self) -> DatabaseId;

    fn recovery_namespace_id(&self) -> NamespaceId;

    fn recovery_visible_catalog_version(&self) -> CatalogVersion;

    fn apply_recovered_catalog_mutation(
        &mut self,
        boundary: &CatalogMutationBoundary,
        deltas: &[CatalogMutationDelta],
    ) -> AndromedaResult<()>;

    fn validate_recovery_boundary_identity(
        &self,
        boundary: &CatalogMutationBoundary,
    ) -> AndromedaResult<()> {
        if boundary.database_id != self.recovery_database_id()
            || boundary.namespace_id != self.recovery_namespace_id()
        {
            return catalog_recovery_error(
                "catalog recovery target identity must match mutation boundary",
            );
        }
        if boundary.previous_version != self.recovery_visible_catalog_version() {
            return catalog_recovery_error(
                "catalog recovery target version must match mutation previous version",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogMutationRecord {
    Begin(CatalogMutationBoundary),
    Apply(Box<CatalogMutationDelta>),
    Commit(CatalogMutationBoundary),
}

impl CatalogMutationRecord {
    pub fn kind(&self) -> CatalogMutationRecordKind {
        match self {
            Self::Begin(_) => CatalogMutationRecordKind::CatalogChangeBegin,
            Self::Apply(_) => CatalogMutationRecordKind::CatalogChangeApply,
            Self::Commit(_) => CatalogMutationRecordKind::CatalogChangeCommit,
        }
    }

    pub fn validate_for_durable_payload(&self) -> AndromedaResult<()> {
        validate_record(self)
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

pub(crate) fn validate_record(record: &CatalogMutationRecord) -> AndromedaResult<()> {
    match record {
        CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
            validate_boundary(boundary)
        },
        CatalogMutationRecord::Apply(delta) => validate_delta(delta),
    }
}

pub(crate) fn validate_boundary(boundary: &CatalogMutationBoundary) -> AndromedaResult<()> {
    if boundary.batch_id.get() == 0 {
        return catalog_recovery_error("catalog WAL boundary batch id must not be zero");
    }
    if boundary.database_id.get() == 0 {
        return catalog_recovery_error("catalog WAL boundary database id must not be zero");
    }
    if boundary.namespace_id.get() == 0 {
        return catalog_recovery_error("catalog WAL boundary namespace id must not be zero");
    }
    if boundary.next_version.get() <= boundary.previous_version.get() {
        return catalog_recovery_error("catalog WAL boundary must advance the catalog version");
    }
    if boundary.source_hash.is_zero() {
        return catalog_recovery_error("catalog WAL boundary source hash must not be zero");
    }
    if boundary.dependency_graph_hash.is_zero() {
        return catalog_recovery_error(
            "catalog WAL boundary dependency graph hash must not be zero",
        );
    }
    if boundary.expected_apply_count == 0 {
        return catalog_recovery_error(
            "catalog WAL boundary expected apply count must not be zero",
        );
    }
    if boundary.expected_apply_count > CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH {
        return catalog_recovery_error(format!(
            "catalog WAL boundary expected apply count must not exceed {CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH}"
        ));
    }
    if boundary.publication_semantics != CatalogPublicationSemantics::DurablePublicationExternal {
        return catalog_recovery_error(
            "catalog WAL boundary requires durable publication semantics",
        );
    }
    Ok(())
}

pub(crate) fn validate_delta(delta: &CatalogMutationDelta) -> AndromedaResult<()> {
    if delta.planned_version.get() == 0 {
        return catalog_recovery_error("catalog WAL delta planned version must not be zero");
    }
    match &delta.operation {
        CatalogMutationOperation::CreateObject { object, definition } => {
            definition.validate()?;
            if *object != *definition.object_ref() {
                return catalog_recovery_error(
                    "catalog WAL create delta object must match its definition object",
                );
            }
            if object.catalog_version != delta.planned_version {
                return catalog_recovery_error(
                    "catalog WAL create delta version must match its object version",
                );
            }
        },
        CatalogMutationOperation::DeprecateObject { target } => {
            target.validate()?;
        },
    }
    Ok(())
}

fn catalog_recovery_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Catalog, message))
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

    #[test]
    fn dependency_graph_hash_preserves_bytes_and_zero_state() {
        let zero = DefinitionBatchDependencyGraphHash::default();
        assert!(zero.is_zero());
        assert_eq!(
            zero.as_bytes(),
            [0; DefinitionBatchDependencyGraphHash::LEN]
        );

        let hash = DefinitionBatchDependencyGraphHash::new([7; 32]);
        assert!(!hash.is_zero());
        assert_eq!(
            hash.as_bytes(),
            [7; DefinitionBatchDependencyGraphHash::LEN]
        );
    }

    #[test]
    fn mutation_record_kind_tags_are_stable() {
        let cases = [
            (
                CatalogMutationRecordKind::CatalogChangeBegin,
                CATALOG_CHANGE_BEGIN_WAL_KIND_TAG,
            ),
            (
                CatalogMutationRecordKind::CatalogChangeApply,
                CATALOG_CHANGE_APPLY_WAL_KIND_TAG,
            ),
            (
                CatalogMutationRecordKind::CatalogChangeCommit,
                CATALOG_CHANGE_COMMIT_WAL_KIND_TAG,
            ),
        ];

        for (kind, tag) in cases {
            assert_eq!(kind.storage_wal_kind_tag(), tag);
            assert_eq!(
                CatalogMutationRecordKind::from_storage_wal_kind_tag(tag),
                Some(kind)
            );
        }
        assert_eq!(
            CatalogMutationRecordKind::from_storage_wal_kind_tag(999),
            None
        );
    }

    #[derive(Debug)]
    struct DummyRecoveryTarget {
        database_id: DatabaseId,
        namespace_id: NamespaceId,
        visible_version: CatalogVersion,
    }

    impl CatalogRecoveryApplyTarget for DummyRecoveryTarget {
        fn recovery_database_id(&self) -> DatabaseId {
            self.database_id
        }

        fn recovery_namespace_id(&self) -> NamespaceId {
            self.namespace_id
        }

        fn recovery_visible_catalog_version(&self) -> CatalogVersion {
            self.visible_version
        }

        fn apply_recovered_catalog_mutation(
            &mut self,
            _boundary: &CatalogMutationBoundary,
            _deltas: &[CatalogMutationDelta],
        ) -> AndromedaResult<()> {
            Ok(())
        }
    }

    #[test]
    fn recovery_apply_target_validates_boundary_identity_without_snapshot_ownership() {
        let target = DummyRecoveryTarget {
            database_id: DatabaseId::new(10),
            namespace_id: NamespaceId::new(20),
            visible_version: CatalogVersion::new(3),
        };
        let boundary = CatalogMutationBoundary {
            batch_id: DefinitionBatchId::new(1),
            database_id: target.database_id,
            namespace_id: target.namespace_id,
            previous_version: target.visible_version,
            next_version: CatalogVersion::new(4),
            source_hash: DefinitionBatchSourceHash::new([1; DefinitionBatchSourceHash::LEN]),
            dependency_graph_hash: DefinitionBatchDependencyGraphHash::new([2; 32]),
            expected_apply_count: 1,
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
        };

        target
            .validate_recovery_boundary_identity(&boundary)
            .expect("matching recovery target accepts boundary identity");

        let mut stale_boundary = boundary;
        stale_boundary.previous_version = CatalogVersion::new(2);
        let err = target
            .validate_recovery_boundary_identity(&stale_boundary)
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Catalog);
        assert!(err.message().contains("previous version"));
    }
}
