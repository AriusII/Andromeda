//! Header framing and version compatibility for catalog WAL payloads.

use crate::{
    CatalogMutationRecordKind, CatalogWalPayloadDecodeError, CatalogWalPayloadDecodeErrorKind,
};

use super::super::constants::{
    CATALOG_WAL_PAYLOAD_HEADER_LEN, CATALOG_WAL_PAYLOAD_MAGIC, CATALOG_WAL_PAYLOAD_VERSION_CURRENT,
    CATALOG_WAL_PAYLOAD_VERSION_V1, CATALOG_WAL_PAYLOAD_VERSION_V2, CATALOG_WAL_PAYLOAD_VERSION_V3,
    CATALOG_WAL_PAYLOAD_VERSION_V4,
};
use super::{
    Decoder, catalog_wal_payload_checksum,
    fields::{push_u16, push_u64},
};

pub(super) struct PayloadHeader {
    pub(super) version: u16,
    pub(super) kind: CatalogMutationRecordKind,
    pub(super) kind_tag: u16,
    pub(super) body_len: u64,
    checksum: u64,
}

pub(super) fn encode_payload_header(
    out: &mut Vec<u8>,
    kind_tag: u16,
    body_len: u64,
    checksum: u64,
) {
    push_u64(out, CATALOG_WAL_PAYLOAD_MAGIC);
    push_u16(out, CATALOG_WAL_PAYLOAD_VERSION_CURRENT);
    push_u16(out, kind_tag);
    push_u64(out, body_len);
    push_u64(out, checksum);
}

pub(super) fn decode_payload_header(
    payload: &[u8],
) -> Result<PayloadHeader, CatalogWalPayloadDecodeError> {
    if payload.len() < CATALOG_WAL_PAYLOAD_HEADER_LEN {
        return Err(CatalogWalPayloadDecodeError::new(
            CatalogWalPayloadDecodeErrorKind::TruncatedHeader,
            "truncated catalog WAL payload header",
        ));
    }

    let mut decoder = Decoder::new(payload);
    let magic = decoder
        .u64()
        .map_err(CatalogWalPayloadDecodeError::from_body_error)?;
    if magic != CATALOG_WAL_PAYLOAD_MAGIC {
        return Err(CatalogWalPayloadDecodeError::new(
            CatalogWalPayloadDecodeErrorKind::MagicMismatch,
            "catalog WAL payload magic mismatch",
        ));
    }

    let version = decoder
        .u16()
        .map_err(CatalogWalPayloadDecodeError::from_body_error)?;
    validate_payload_version(version)?;

    let kind_tag = decoder
        .u16()
        .map_err(CatalogWalPayloadDecodeError::from_body_error)?;
    let kind = CatalogMutationRecordKind::from_storage_wal_kind_tag(kind_tag).ok_or_else(|| {
        CatalogWalPayloadDecodeError::new(
            CatalogWalPayloadDecodeErrorKind::UnknownRecordKindTag,
            "unknown catalog WAL record kind tag",
        )
    })?;
    let body_len = decoder
        .u64()
        .map_err(CatalogWalPayloadDecodeError::from_body_error)?;
    let checksum = decoder
        .u64()
        .map_err(CatalogWalPayloadDecodeError::from_body_error)?;

    Ok(PayloadHeader {
        version,
        kind,
        kind_tag,
        body_len,
        checksum,
    })
}

pub(super) fn payload_body<'a>(
    payload: &'a [u8],
    header: &PayloadHeader,
) -> Result<&'a [u8], CatalogWalPayloadDecodeError> {
    let body_len_usize = usize::try_from(header.body_len).map_err(|_| {
        CatalogWalPayloadDecodeError::new(
            CatalogWalPayloadDecodeErrorKind::BodyLengthOverflow,
            "catalog WAL payload body length does not fit usize",
        )
    })?;
    if payload.len() - CATALOG_WAL_PAYLOAD_HEADER_LEN != body_len_usize {
        return Err(CatalogWalPayloadDecodeError::new(
            CatalogWalPayloadDecodeErrorKind::BodyLengthMismatch,
            "catalog WAL payload body length mismatch",
        ));
    }

    Ok(&payload[CATALOG_WAL_PAYLOAD_HEADER_LEN..])
}

pub(super) fn verify_payload_checksum(
    header: &PayloadHeader,
    body: &[u8],
) -> Result<(), CatalogWalPayloadDecodeError> {
    if header.checksum
        != catalog_wal_payload_checksum(header.version, header.kind_tag, header.body_len, body)
    {
        return Err(CatalogWalPayloadDecodeError::new(
            CatalogWalPayloadDecodeErrorKind::ChecksumMismatch,
            "catalog WAL payload checksum mismatch",
        ));
    }
    Ok(())
}

fn validate_payload_version(version: u16) -> Result<(), CatalogWalPayloadDecodeError> {
    if matches!(
        version,
        CATALOG_WAL_PAYLOAD_VERSION_V1
            | CATALOG_WAL_PAYLOAD_VERSION_V2
            | CATALOG_WAL_PAYLOAD_VERSION_V3
    ) {
        return Err(CatalogWalPayloadDecodeError::new(
            CatalogWalPayloadDecodeErrorKind::LegacyFormatVersion,
            "legacy catalog WAL payload lacks complete DefinitionBatch replay integrity",
        ));
    }
    if version != CATALOG_WAL_PAYLOAD_VERSION_V4 {
        return Err(CatalogWalPayloadDecodeError::new(
            CatalogWalPayloadDecodeErrorKind::UnsupportedFormatVersion,
            "unsupported catalog WAL payload version",
        ));
    }
    Ok(())
}
