use crate::{
    CatalogMutationRecord, CatalogWalPayloadDecodeErrorKind, DefinitionBatchId,
    recovery::CatalogRecoveryAnomalyKind,
};

pub(super) fn recovery_anomaly_kind_for_decode_error(
    kind: CatalogWalPayloadDecodeErrorKind,
) -> CatalogRecoveryAnomalyKind {
    match kind {
        CatalogWalPayloadDecodeErrorKind::MagicMismatch => {
            CatalogRecoveryAnomalyKind::PayloadMagicMismatch
        }
        CatalogWalPayloadDecodeErrorKind::LegacyFormatVersion
        | CatalogWalPayloadDecodeErrorKind::UnsupportedFormatVersion => {
            CatalogRecoveryAnomalyKind::PayloadFormatVersionMismatch
        }
        CatalogWalPayloadDecodeErrorKind::ChecksumMismatch => {
            CatalogRecoveryAnomalyKind::PayloadChecksumMismatch
        }
        CatalogWalPayloadDecodeErrorKind::UnknownRecordKindTag => {
            CatalogRecoveryAnomalyKind::WrongKindTag
        }
        CatalogWalPayloadDecodeErrorKind::TruncatedHeader
        | CatalogWalPayloadDecodeErrorKind::BodyLengthOverflow
        | CatalogWalPayloadDecodeErrorKind::BodyLengthMismatch
        | CatalogWalPayloadDecodeErrorKind::BodyInvalid => {
            CatalogRecoveryAnomalyKind::PayloadCorruption
        }
    }
}

pub(super) fn boundary_batch_id(record: &CatalogMutationRecord) -> Option<DefinitionBatchId> {
    match record {
        CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
            Some(boundary.batch_id)
        }
        CatalogMutationRecord::Apply(_) => None,
    }
}
