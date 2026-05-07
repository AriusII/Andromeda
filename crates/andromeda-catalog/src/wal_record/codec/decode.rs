//! Body decoding for catalog mutation WAL payloads.

use andromeda_core::{
    AbsencePolicy, AndromedaResult, CatalogObjectId, CatalogVersion, ColumnDescriptor,
    ContractHash, DatabaseId, DecimalType, FloatMode, FloatType, NamespaceId, ProcedureId,
    ScalarType, TextEncoding, TextType, TimestampType, TypeDescriptor,
};

use crate::{
    AccessMode, CatalogDefinition, CatalogLifecycleTarget, CatalogMutationBoundary,
    CatalogMutationDelta, CatalogMutationOperation, CatalogMutationRecord,
    CatalogMutationRecordKind, CatalogObjectRef, CatalogPublicationSemantics,
    CatalogWalPayloadDecodeError, CompatibilityPolicy, DefinitionBatchDependencyGraphHash,
    DefinitionBatchSourceHash, EnumDefinition, EnumVariant, IsolationPolicy, MultiResultPolicy,
    ObjectKind, ProcedureContract, ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName,
    ResultMetadataPolicy, ResultStreamCardinality, ResultStreamContract, StatsVersion,
    StructuredObjectDefinition, TableDefinition, TransactionPolicy,
};

use super::super::constants::CATALOG_WAL_PAYLOAD_VERSION_V2;
use super::{
    Decoder, catalog_error,
    format::{decode_payload_header, payload_body, verify_payload_checksum},
    validation::{validate_boundary, validate_delta},
};

pub(super) fn decode_durable_payload(
    payload: &[u8],
) -> Result<CatalogMutationRecord, CatalogWalPayloadDecodeError> {
    let header = decode_payload_header(payload)?;
    let body = payload_body(payload, &header)?;
    verify_payload_checksum(&header, body)?;

    let mut body_decoder = Decoder::new(body);
    let record = match header.kind {
        CatalogMutationRecordKind::CatalogChangeBegin => CatalogMutationRecord::Begin(
            decode_boundary(&mut body_decoder)
                .map_err(CatalogWalPayloadDecodeError::from_body_error)?,
        ),
        CatalogMutationRecordKind::CatalogChangeApply => CatalogMutationRecord::Apply(Box::new(
            decode_delta(&mut body_decoder, header.version)
                .map_err(CatalogWalPayloadDecodeError::from_body_error)?,
        )),
        CatalogMutationRecordKind::CatalogChangeCommit => CatalogMutationRecord::Commit(
            decode_boundary(&mut body_decoder)
                .map_err(CatalogWalPayloadDecodeError::from_body_error)?,
        ),
    };
    body_decoder
        .finish()
        .map_err(CatalogWalPayloadDecodeError::from_body_error)?;
    record
        .validate_for_durable_payload()
        .map_err(CatalogWalPayloadDecodeError::from_body_error)?;
    Ok(record)
}

fn decode_boundary(decoder: &mut Decoder<'_>) -> AndromedaResult<CatalogMutationBoundary> {
    let boundary = CatalogMutationBoundary {
        batch_id: crate::DefinitionBatchId::new(decoder.u64()?),
        database_id: DatabaseId::new(decoder.u64()?),
        namespace_id: NamespaceId::new(decoder.u64()?),
        previous_version: CatalogVersion::new(decoder.u64()?),
        next_version: CatalogVersion::new(decoder.u64()?),
        source_hash: DefinitionBatchSourceHash::new(decode_sha256_array(decoder)?),
        dependency_graph_hash: DefinitionBatchDependencyGraphHash::new(decode_sha256_array(
            decoder,
        )?),
        expected_apply_count: usize::try_from(decoder.u64()?).map_err(|_| {
            catalog_error("catalog WAL boundary expected apply count does not fit usize")
        })?,
        publication_semantics: match decoder.u8()? {
            0 => CatalogPublicationSemantics::PlannedVersionOnly,
            1 => CatalogPublicationSemantics::DurablePublicationExternal,
            _ => return Err(catalog_error("unknown catalog publication semantics tag")),
        },
    };
    validate_boundary(&boundary)?;
    Ok(boundary)
}

fn decode_sha256_array(decoder: &mut Decoder<'_>) -> AndromedaResult<[u8; 32]> {
    decoder
        .bytes(32)?
        .try_into()
        .map_err(|_| catalog_error("catalog WAL SHA-256 field has invalid width"))
}

fn decode_delta(
    decoder: &mut Decoder<'_>,
    payload_version: u16,
) -> AndromedaResult<CatalogMutationDelta> {
    let operation_index = usize::try_from(decoder.u64()?)
        .map_err(|_| catalog_error("catalog WAL delta operation index does not fit usize"))?;
    let planned_version = CatalogVersion::new(decoder.u64()?);
    let operation = match decoder.u8()? {
        0 => CatalogMutationOperation::CreateObject {
            object: decode_object_ref(decoder)?,
            definition: decode_definition(decoder, payload_version)?,
        },
        1 => CatalogMutationOperation::DeprecateObject {
            target: decode_lifecycle_target(decoder)?,
        },
        _ => return Err(catalog_error("unknown catalog WAL delta operation tag")),
    };
    let delta = CatalogMutationDelta {
        operation_index,
        planned_version,
        operation,
    };
    validate_delta(&delta)?;
    Ok(delta)
}

fn decode_lifecycle_target(decoder: &mut Decoder<'_>) -> AndromedaResult<CatalogLifecycleTarget> {
    Ok(CatalogLifecycleTarget {
        object: decode_object_ref(decoder)?,
    })
}

fn decode_definition(
    decoder: &mut Decoder<'_>,
    payload_version: u16,
) -> AndromedaResult<CatalogDefinition> {
    match decoder.u8()? {
        0 => Ok(CatalogDefinition::Table(decode_table(decoder)?)),
        1 => Ok(CatalogDefinition::StructuredObject(
            decode_structured_object(decoder)?,
        )),
        2 => Ok(CatalogDefinition::Enum(decode_enum(decoder)?)),
        3 => Ok(CatalogDefinition::Procedure(decode_procedure(
            decoder,
            payload_version,
        )?)),
        _ => Err(catalog_error("unknown catalog definition tag")),
    }
}

fn decode_table(decoder: &mut Decoder<'_>) -> AndromedaResult<TableDefinition> {
    Ok(TableDefinition {
        object: decode_object_ref(decoder)?,
        columns: decode_columns(decoder)?,
    })
}

fn decode_structured_object(
    decoder: &mut Decoder<'_>,
) -> AndromedaResult<StructuredObjectDefinition> {
    Ok(StructuredObjectDefinition {
        object: decode_object_ref(decoder)?,
        fields: decode_columns(decoder)?,
        unique_by: decode_strings(decoder)?,
    })
}

fn decode_enum(decoder: &mut Decoder<'_>) -> AndromedaResult<EnumDefinition> {
    let object = decode_object_ref(decoder)?;
    let count = decoder.len()?;
    let mut variants = Vec::with_capacity(count);
    for _ in 0..count {
        variants.push(EnumVariant {
            name: decoder.string()?,
            value: decoder.i64()?,
        });
    }
    Ok(EnumDefinition { object, variants })
}

fn decode_procedure(
    decoder: &mut Decoder<'_>,
    payload_version: u16,
) -> AndromedaResult<ProcedureContract> {
    let object = decode_object_ref(decoder)?;
    let procedure_id = ProcedureId::new(decoder.u64()?);
    let contract_hash = ContractHash::from_slice(decoder.bytes(ContractHash::LEN)?)?;
    let stats_version = StatsVersion::new(decoder.u64()?);
    let protocol_layout = ProtocolLayoutRef {
        descriptor_set_hash: ContractHash::from_slice(decoder.bytes(ContractHash::LEN)?)?,
        frame_envelope_hash: ContractHash::from_slice(decoder.bytes(ContractHash::LEN)?)?,
    };
    let inputs = decode_columns(decoder)?;
    let structured_inputs = decode_qualified_names(decoder)?;
    let result_streams = decode_result_streams(decoder, payload_version)?;
    let required_permissions = decode_strings(decoder)?;
    let transaction_policy = decode_transaction_policy(decoder)?;
    let compatibility_policy = match decoder.u8()? {
        0 => CompatibilityPolicy::AdditiveOnly,
        1 => CompatibilityPolicy::ExactHash,
        _ => return Err(catalog_error("unknown compatibility policy tag")),
    };
    let result_metadata_policy = match decoder.u8()? {
        0 => ResultMetadataPolicy::RequireBeforePayload,
        1 => ResultMetadataPolicy::AllowStreamingUnknown,
        _ => return Err(catalog_error("unknown result metadata policy tag")),
    };
    let error_policy = ProcedureErrorPolicy {
        rollback_on_error: decoder.bool()?,
        allowed_error_codes: decode_strings(decoder)?,
    };
    let multi_result_policy = match decoder.u8()? {
        0 => MultiResultPolicy::SingleResultOnly,
        1 => MultiResultPolicy::MultipleResultStreamsAllowed,
        _ => return Err(catalog_error("unknown multi-result policy tag")),
    };

    Ok(ProcedureContract {
        object,
        procedure_id,
        contract_hash,
        stats_version,
        protocol_layout,
        inputs,
        structured_inputs,
        result_streams,
        required_permissions,
        transaction_policy,
        compatibility_policy,
        result_metadata_policy,
        error_policy,
        multi_result_policy,
    })
}

fn decode_result_streams(
    decoder: &mut Decoder<'_>,
    payload_version: u16,
) -> AndromedaResult<Vec<ResultStreamContract>> {
    let count = decoder.len()?;
    let mut streams = Vec::with_capacity(count);
    for _ in 0..count {
        let stream_id = decoder.u64()?;
        let name = decoder.string()?;
        let columns = decode_columns(decoder)?;
        let row_count_exact_required = decoder.bool()?;
        let cardinality = if payload_version >= CATALOG_WAL_PAYLOAD_VERSION_V2 {
            ResultStreamCardinality::from_stable_tag(decoder.u8()?)?
        } else {
            ResultStreamCardinality::from_legacy_row_count_exact_required(row_count_exact_required)
        };
        streams.push(ResultStreamContract {
            stream_id,
            name,
            columns,
            cardinality,
            row_count_exact_required,
        });
    }
    Ok(streams)
}

fn decode_transaction_policy(decoder: &mut Decoder<'_>) -> AndromedaResult<TransactionPolicy> {
    let access_mode = match decoder.u8()? {
        0 => AccessMode::ReadOnly,
        1 => AccessMode::ReadWrite,
        _ => return Err(catalog_error("unknown access mode tag")),
    };
    let isolation = match decoder.u8()? {
        0 => IsolationPolicy::Snapshot,
        1 => IsolationPolicy::Serializable,
        _ => return Err(catalog_error("unknown isolation policy tag")),
    };
    Ok(TransactionPolicy {
        access_mode,
        isolation,
        retryable: decoder.bool()?,
    })
}

fn decode_object_ref(decoder: &mut Decoder<'_>) -> AndromedaResult<CatalogObjectRef> {
    Ok(CatalogObjectRef {
        object_id: CatalogObjectId::new(decoder.u64()?),
        name: decode_qualified_name(decoder)?,
        kind: object_kind_from_tag(decoder.u8()?)?,
        catalog_version: CatalogVersion::new(decoder.u64()?),
    })
}

fn object_kind_from_tag(tag: u8) -> AndromedaResult<ObjectKind> {
    match tag {
        0 => Ok(ObjectKind::Database),
        1 => Ok(ObjectKind::Namespace),
        2 => Ok(ObjectKind::Table),
        3 => Ok(ObjectKind::Map),
        4 => Ok(ObjectKind::Enum),
        5 => Ok(ObjectKind::StructuredObject),
        6 => Ok(ObjectKind::Procedure),
        _ => Err(catalog_error("unknown object kind tag")),
    }
}

fn decode_columns(decoder: &mut Decoder<'_>) -> AndromedaResult<Vec<ColumnDescriptor>> {
    let count = decoder.len()?;
    let mut columns = Vec::with_capacity(count);
    for _ in 0..count {
        columns.push(ColumnDescriptor {
            name: decoder.string()?,
            data_type: decode_type_descriptor(decoder)?,
            ordinal: decoder.u32()?,
        });
    }
    Ok(columns)
}

fn decode_type_descriptor(decoder: &mut Decoder<'_>) -> AndromedaResult<TypeDescriptor> {
    let scalar = decode_scalar_type(decoder)?;
    let absence = match decoder.u8()? {
        0 => AbsencePolicy::Required,
        1 => AbsencePolicy::ExplicitOptional,
        _ => return Err(catalog_error("unknown absence policy tag")),
    };
    Ok(TypeDescriptor { scalar, absence })
}

fn decode_scalar_type(decoder: &mut Decoder<'_>) -> AndromedaResult<ScalarType> {
    match decoder.u8()? {
        0 => Ok(ScalarType::I8),
        1 => Ok(ScalarType::I16),
        2 => Ok(ScalarType::I32),
        3 => Ok(ScalarType::I64),
        4 => Ok(ScalarType::I128),
        5 => Ok(ScalarType::U8),
        6 => Ok(ScalarType::U16),
        7 => Ok(ScalarType::U32),
        8 => Ok(ScalarType::U64),
        9 => Ok(ScalarType::U128),
        10 => Ok(ScalarType::Decimal(decode_decimal_type(decoder)?)),
        11 => Ok(ScalarType::Float(decode_float_type(decoder)?)),
        12 => Ok(ScalarType::Bool),
        13 => {
            let encoding = match decoder.u8()? {
                0 => TextEncoding::Utf8,
                1 => TextEncoding::Utf16,
                2 => TextEncoding::Unicode,
                _ => return Err(catalog_error("unknown text encoding tag")),
            };
            let max_length = if decoder.bool()? {
                Some(decoder.u32()?)
            } else {
                None
            };
            let collation = if decoder.bool()? {
                Some(decoder.string()?)
            } else {
                None
            };
            Ok(ScalarType::Text(TextType {
                encoding,
                max_length,
                collation,
            }))
        }
        14 => {
            let timestamp = match decoder.u8()? {
                0 => TimestampType::Transaction,
                1 => TimestampType::Invocation,
                2 => TimestampType::MonotonicEpoch,
                _ => return Err(catalog_error("unknown timestamp type tag")),
            };
            Ok(ScalarType::Timestamp(timestamp))
        }
        _ => Err(catalog_error("unknown scalar type tag")),
    }
}

fn decode_decimal_type(decoder: &mut Decoder<'_>) -> AndromedaResult<DecimalType> {
    match decoder.u8()? {
        0 => Ok(DecimalType::Min),
        1 => Ok(DecimalType::Mid),
        2 => Ok(DecimalType::Max),
        3 => Ok(DecimalType::Custom {
            precision: decoder.u8()?,
            scale: decoder.u8()?,
        }),
        _ => Err(catalog_error("unknown decimal type tag")),
    }
}

fn decode_float_type(decoder: &mut Decoder<'_>) -> AndromedaResult<FloatType> {
    match decoder.u8()? {
        0 => Ok(FloatType::Min),
        1 => Ok(FloatType::Mid),
        2 => Ok(FloatType::Max),
        3 => {
            let bits = decoder.u16()?;
            let mode = match decoder.u8()? {
                0 => FloatMode::Approximate,
                1 => FloatMode::DeterministicAnalytics,
                _ => return Err(catalog_error("unknown float mode tag")),
            };
            Ok(FloatType::Custom { bits, mode })
        }
        _ => Err(catalog_error("unknown float type tag")),
    }
}

fn decode_qualified_names(decoder: &mut Decoder<'_>) -> AndromedaResult<Vec<QualifiedName>> {
    let count = decoder.len()?;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        names.push(decode_qualified_name(decoder)?);
    }
    Ok(names)
}

fn decode_qualified_name(decoder: &mut Decoder<'_>) -> AndromedaResult<QualifiedName> {
    let count = decoder.len()?;
    let mut parts = Vec::with_capacity(count);
    for _ in 0..count {
        parts.push(decoder.string()?);
    }
    QualifiedName::new(parts)
}

fn decode_strings(decoder: &mut Decoder<'_>) -> AndromedaResult<Vec<String>> {
    let count = decoder.len()?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(decoder.string()?);
    }
    Ok(values)
}
