use andromeda_catalog_store::CatalogStoreMutationKind;
use andromeda_catalog_store::{CatalogDefinition, CatalogObjectRef};
use andromeda_definition_batch::{DefinitionBatchId, DefinitionBatchSourceHash};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

pub const CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH: usize = 1024;

pub const CATALOG_CHANGE_BEGIN_WAL_KIND_TAG: u16 = 19;
pub const CATALOG_CHANGE_APPLY_WAL_KIND_TAG: u16 = 20;
pub const CATALOG_CHANGE_COMMIT_WAL_KIND_TAG: u16 = 21;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DefinitionBatchDependencyGraphHash([u8; Self::LEN]);

impl DefinitionBatchDependencyGraphHash {
    pub const LEN: usize = 32;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationSemantics {
    PlannedVersionOnly,
    DurablePublicationExternal,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogMutationBoundary {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub source_hash: DefinitionBatchSourceHash,
    pub dependency_graph_hash: DefinitionBatchDependencyGraphHash,
    pub expected_apply_count: usize,
    pub publication_semantics: CatalogPublicationSemantics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogLifecycleTarget {
    pub object: CatalogObjectRef,
}

impl CatalogLifecycleTarget {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.object.validate_for_definition(self.object.kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogMutationDelta {
    pub operation_index: usize,
    pub planned_version: CatalogVersion,
    pub operation: CatalogMutationOperation,
}

impl CatalogMutationDelta {
    pub fn create(
        operation_index: usize,
        planned_version: CatalogVersion,
        definition: CatalogDefinition,
    ) -> Self {
        let object = definition.object_ref().clone();
        Self {
            operation_index,
            planned_version,
            operation: CatalogMutationOperation::CreateObject { object, definition },
        }
    }

    pub fn deprecate(
        operation_index: usize,
        planned_version: CatalogVersion,
        target: CatalogLifecycleTarget,
    ) -> Self {
        Self {
            operation_index,
            planned_version,
            operation: CatalogMutationOperation::DeprecateObject { target },
        }
    }

    pub fn object(&self) -> &CatalogObjectRef {
        match &self.operation {
            CatalogMutationOperation::CreateObject { object, .. } => object,
            CatalogMutationOperation::DeprecateObject { target } => &target.object,
        }
    }

    pub fn definition(&self) -> Option<&CatalogDefinition> {
        match &self.operation {
            CatalogMutationOperation::CreateObject { definition, .. } => Some(definition),
            CatalogMutationOperation::DeprecateObject { .. } => None,
        }
    }
}

#[allow(
    clippy::large_enum_variant,
    reason = "Catalog WAL mutation payloads stay inline to preserve deterministic value semantics."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogMutationOperation {
    CreateObject {
        object: CatalogObjectRef,
        definition: CatalogDefinition,
    },
    DeprecateObject {
        target: CatalogLifecycleTarget,
    },
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
            if object != definition.object_ref() {
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
}
