//! Durable catalog mutation record payload codec.
//!
//! Catalog mutation durability is represented as one binary payload per
//! [`CatalogMutationRecord`].  The storage WAL remains the owner of LSNs,
//! transaction ids, frame checksums, sync policy, and replay ordering.  This
//! module defines the catalog-local durable payload that is embedded in the
//! storage WAL records whose stable kind tags are:
//!
//! - `19` / `CatalogChangeBegin`
//! - `20` / `CatalogChangeApply`
//! - `21` / `CatalogChangeCommit`
//!
//! The codec is intentionally binary, versioned, little-endian, and independent
//! of JSON or ad hoc SQL text so later publication/recovery work can consume the
//! same durable boundary without changing the storage WAL frame format.

use andromeda_core::{
    AbsencePolicy, AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId,
    CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId, DecimalType, FloatMode, FloatType,
    NamespaceId, ProcedureId, ScalarType, TextEncoding, TextType, TimestampType, TypeDescriptor,
};

use crate::{
    AccessMode, CatalogDefinition, CatalogLifecycleTarget, CatalogMutationBoundary,
    CatalogMutationDelta, CatalogMutationOperation, CatalogMutationRecord,
    CatalogMutationRecordKind, CatalogObjectRef, CatalogPublicationSemantics, CompatibilityPolicy,
    EnumDefinition, EnumVariant, IsolationPolicy, MultiResultPolicy, ObjectKind, ProcedureContract,
    ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy,
    ResultStreamContract, StatsVersion, StructuredObjectDefinition, TableDefinition,
    TransactionPolicy,
};

const CATALOG_WAL_PAYLOAD_MAGIC: u64 = 0x414e_4452_4341_5457; // "ANDRCATW"
const CATALOG_WAL_PAYLOAD_VERSION_V1: u16 = 1;
const CATALOG_WAL_PAYLOAD_HEADER_LEN: usize = 28;

/// Stable storage WAL kind tag for `CatalogChangeBegin`.
pub const CATALOG_CHANGE_BEGIN_WAL_KIND_TAG: u16 = 19;
/// Stable storage WAL kind tag for `CatalogChangeApply`.
pub const CATALOG_CHANGE_APPLY_WAL_KIND_TAG: u16 = 20;
/// Stable storage WAL kind tag for `CatalogChangeCommit`.
pub const CATALOG_CHANGE_COMMIT_WAL_KIND_TAG: u16 = 21;

impl CatalogMutationRecordKind {
    /// Stable storage WAL kind tag that must wrap this catalog payload.
    pub const fn storage_wal_kind_tag(self) -> u16 {
        match self {
            Self::CatalogChangeBegin => CATALOG_CHANGE_BEGIN_WAL_KIND_TAG,
            Self::CatalogChangeApply => CATALOG_CHANGE_APPLY_WAL_KIND_TAG,
            Self::CatalogChangeCommit => CATALOG_CHANGE_COMMIT_WAL_KIND_TAG,
        }
    }

    pub(crate) const fn from_storage_wal_kind_tag(tag: u16) -> Option<Self> {
        match tag {
            CATALOG_CHANGE_BEGIN_WAL_KIND_TAG => Some(Self::CatalogChangeBegin),
            CATALOG_CHANGE_APPLY_WAL_KIND_TAG => Some(Self::CatalogChangeApply),
            CATALOG_CHANGE_COMMIT_WAL_KIND_TAG => Some(Self::CatalogChangeCommit),
            _ => None,
        }
    }
}

impl CatalogMutationRecord {
    /// Encode this catalog mutation record as a durable storage-WAL payload.
    ///
    /// The caller must wrap the resulting bytes in a storage WAL record whose
    /// kind tag matches [`CatalogMutationRecordKind::storage_wal_kind_tag`].
    pub fn encode_durable_payload(&self) -> AndromedaResult<Vec<u8>> {
        self.validate_for_durable_payload()?;

        let mut body = Vec::new();
        match self {
            Self::Begin(boundary) | Self::Commit(boundary) => encode_boundary(&mut body, boundary),
            Self::Apply(delta) => encode_delta(&mut body, delta),
        }

        let kind_tag = self.kind().storage_wal_kind_tag();
        let body_len = u64::try_from(body.len())
            .map_err(|_| catalog_error("catalog WAL payload body is too large"))?;
        let checksum = catalog_wal_payload_checksum(kind_tag, body_len, &body);

        let mut encoded = Vec::with_capacity(CATALOG_WAL_PAYLOAD_HEADER_LEN + body.len());
        push_u64(&mut encoded, CATALOG_WAL_PAYLOAD_MAGIC);
        push_u16(&mut encoded, CATALOG_WAL_PAYLOAD_VERSION_V1);
        push_u16(&mut encoded, kind_tag);
        push_u64(&mut encoded, body_len);
        push_u64(&mut encoded, checksum);
        encoded.extend_from_slice(&body);
        Ok(encoded)
    }

    /// Decode a durable catalog mutation payload.
    ///
    /// This validates the payload header, body checksum, kind/body agreement,
    /// and catalog-local structural invariants. It does not publish the decoded
    /// mutation or replay it into a snapshot.
    pub fn decode_durable_payload(payload: &[u8]) -> AndromedaResult<Self> {
        if payload.len() < CATALOG_WAL_PAYLOAD_HEADER_LEN {
            return Err(catalog_error("truncated catalog WAL payload header"));
        }

        let mut decoder = Decoder::new(payload);
        let magic = decoder.u64()?;
        if magic != CATALOG_WAL_PAYLOAD_MAGIC {
            return Err(catalog_error("catalog WAL payload magic mismatch"));
        }

        let version = decoder.u16()?;
        if version != CATALOG_WAL_PAYLOAD_VERSION_V1 {
            return Err(catalog_error("unsupported catalog WAL payload version"));
        }

        let kind_tag = decoder.u16()?;
        let kind = CatalogMutationRecordKind::from_storage_wal_kind_tag(kind_tag)
            .ok_or_else(|| catalog_error("unknown catalog WAL record kind tag"))?;
        let body_len = decoder.u64()?;
        let checksum = decoder.u64()?;

        let body_len_usize = usize::try_from(body_len)
            .map_err(|_| catalog_error("catalog WAL payload body length does not fit usize"))?;
        if payload.len() - CATALOG_WAL_PAYLOAD_HEADER_LEN != body_len_usize {
            return Err(catalog_error("catalog WAL payload body length mismatch"));
        }

        let body = &payload[CATALOG_WAL_PAYLOAD_HEADER_LEN..];
        if checksum != catalog_wal_payload_checksum(kind_tag, body_len, body) {
            return Err(catalog_error("catalog WAL payload checksum mismatch"));
        }

        let mut body_decoder = Decoder::new(body);
        let record = match kind {
            CatalogMutationRecordKind::CatalogChangeBegin => {
                Self::Begin(decode_boundary(&mut body_decoder)?)
            }
            CatalogMutationRecordKind::CatalogChangeApply => {
                Self::Apply(Box::new(decode_delta(&mut body_decoder)?))
            }
            CatalogMutationRecordKind::CatalogChangeCommit => {
                Self::Commit(decode_boundary(&mut body_decoder)?)
            }
        };
        body_decoder.finish()?;
        record.validate_for_durable_payload()?;
        Ok(record)
    }

    pub fn validate_for_durable_payload(&self) -> AndromedaResult<()> {
        match self {
            Self::Begin(boundary) | Self::Commit(boundary) => validate_boundary(boundary),
            Self::Apply(delta) => validate_delta(delta),
        }
    }
}

fn validate_boundary(boundary: &CatalogMutationBoundary) -> AndromedaResult<()> {
    if boundary.batch_id.get() == 0 {
        return Err(catalog_error(
            "catalog WAL boundary batch id must not be zero",
        ));
    }
    if boundary.database_id.get() == 0 {
        return Err(catalog_error(
            "catalog WAL boundary database id must not be zero",
        ));
    }
    if boundary.namespace_id.get() == 0 {
        return Err(catalog_error(
            "catalog WAL boundary namespace id must not be zero",
        ));
    }
    if boundary.next_version.get() <= boundary.previous_version.get() {
        return Err(catalog_error(
            "catalog WAL boundary must advance the catalog version",
        ));
    }
    Ok(())
}

fn validate_delta(delta: &CatalogMutationDelta) -> AndromedaResult<()> {
    if delta.planned_version.get() == 0 {
        return Err(catalog_error(
            "catalog WAL delta planned version must not be zero",
        ));
    }
    match &delta.operation {
        CatalogMutationOperation::CreateObject { object, definition } => {
            definition.validate()?;
            if object != definition.object_ref() {
                return Err(catalog_error(
                    "catalog WAL create delta object must match its definition object",
                ));
            }
            if object.catalog_version != delta.planned_version {
                return Err(catalog_error(
                    "catalog WAL create delta version must match its object version",
                ));
            }
        }
        CatalogMutationOperation::DeprecateObject { target } => {
            target.validate()?;
        }
    }
    Ok(())
}

fn encode_boundary(out: &mut Vec<u8>, boundary: &CatalogMutationBoundary) {
    push_u64(out, boundary.batch_id.get());
    push_u64(out, boundary.database_id.get());
    push_u64(out, boundary.namespace_id.get());
    push_u64(out, boundary.previous_version.get());
    push_u64(out, boundary.next_version.get());
    push_u8(
        out,
        match boundary.publication_semantics {
            CatalogPublicationSemantics::PlannedVersionOnly => 0,
            CatalogPublicationSemantics::DurablePublicationExternal => 1,
        },
    );
}

fn decode_boundary(decoder: &mut Decoder<'_>) -> AndromedaResult<CatalogMutationBoundary> {
    let boundary = CatalogMutationBoundary {
        batch_id: crate::DefinitionBatchId::new(decoder.u64()?),
        database_id: DatabaseId::new(decoder.u64()?),
        namespace_id: NamespaceId::new(decoder.u64()?),
        previous_version: CatalogVersion::new(decoder.u64()?),
        next_version: CatalogVersion::new(decoder.u64()?),
        publication_semantics: match decoder.u8()? {
            0 => CatalogPublicationSemantics::PlannedVersionOnly,
            1 => CatalogPublicationSemantics::DurablePublicationExternal,
            _ => return Err(catalog_error("unknown catalog publication semantics tag")),
        },
    };
    validate_boundary(&boundary)?;
    Ok(boundary)
}

fn encode_delta(out: &mut Vec<u8>, delta: &CatalogMutationDelta) {
    push_u64(out, delta.operation_index as u64);
    push_u64(out, delta.planned_version.get());
    match &delta.operation {
        CatalogMutationOperation::CreateObject { object, definition } => {
            push_u8(out, 0);
            encode_object_ref(out, object);
            encode_definition(out, definition);
        }
        CatalogMutationOperation::DeprecateObject { target } => {
            push_u8(out, 1);
            encode_lifecycle_target(out, target);
        }
    }
}

fn decode_delta(decoder: &mut Decoder<'_>) -> AndromedaResult<CatalogMutationDelta> {
    let operation_index = usize::try_from(decoder.u64()?)
        .map_err(|_| catalog_error("catalog WAL delta operation index does not fit usize"))?;
    let planned_version = CatalogVersion::new(decoder.u64()?);
    let operation = match decoder.u8()? {
        0 => CatalogMutationOperation::CreateObject {
            object: decode_object_ref(decoder)?,
            definition: decode_definition(decoder)?,
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

fn encode_lifecycle_target(out: &mut Vec<u8>, target: &CatalogLifecycleTarget) {
    encode_object_ref(out, &target.object);
}

fn decode_lifecycle_target(decoder: &mut Decoder<'_>) -> AndromedaResult<CatalogLifecycleTarget> {
    Ok(CatalogLifecycleTarget {
        object: decode_object_ref(decoder)?,
    })
}

fn encode_definition(out: &mut Vec<u8>, definition: &CatalogDefinition) {
    match definition {
        CatalogDefinition::Table(table) => {
            push_u8(out, 0);
            encode_table(out, table);
        }
        CatalogDefinition::StructuredObject(object) => {
            push_u8(out, 1);
            encode_structured_object(out, object);
        }
        CatalogDefinition::Enum(enum_definition) => {
            push_u8(out, 2);
            encode_enum(out, enum_definition);
        }
        CatalogDefinition::Procedure(procedure) => {
            push_u8(out, 3);
            encode_procedure(out, procedure);
        }
    }
}

fn decode_definition(decoder: &mut Decoder<'_>) -> AndromedaResult<CatalogDefinition> {
    match decoder.u8()? {
        0 => Ok(CatalogDefinition::Table(decode_table(decoder)?)),
        1 => Ok(CatalogDefinition::StructuredObject(
            decode_structured_object(decoder)?,
        )),
        2 => Ok(CatalogDefinition::Enum(decode_enum(decoder)?)),
        3 => Ok(CatalogDefinition::Procedure(decode_procedure(decoder)?)),
        _ => Err(catalog_error("unknown catalog definition tag")),
    }
}

fn encode_table(out: &mut Vec<u8>, table: &TableDefinition) {
    encode_object_ref(out, &table.object);
    encode_columns(out, &table.columns);
}

fn decode_table(decoder: &mut Decoder<'_>) -> AndromedaResult<TableDefinition> {
    Ok(TableDefinition {
        object: decode_object_ref(decoder)?,
        columns: decode_columns(decoder)?,
    })
}

fn encode_structured_object(out: &mut Vec<u8>, object: &StructuredObjectDefinition) {
    encode_object_ref(out, &object.object);
    encode_columns(out, &object.fields);
    encode_strings(out, &object.unique_by);
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

fn encode_enum(out: &mut Vec<u8>, definition: &EnumDefinition) {
    encode_object_ref(out, &definition.object);
    push_u64(out, definition.variants.len() as u64);
    for variant in &definition.variants {
        encode_string(out, &variant.name);
        push_i64(out, variant.value);
    }
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

fn encode_procedure(out: &mut Vec<u8>, procedure: &ProcedureContract) {
    encode_object_ref(out, &procedure.object);
    push_u64(out, procedure.procedure_id.get());
    out.extend_from_slice(&procedure.contract_hash.as_bytes());
    push_u64(out, procedure.stats_version.get());
    out.extend_from_slice(&procedure.protocol_layout.descriptor_set_hash.as_bytes());
    out.extend_from_slice(&procedure.protocol_layout.frame_envelope_hash.as_bytes());
    encode_columns(out, &procedure.inputs);
    encode_qualified_names(out, &procedure.structured_inputs);
    encode_result_streams(out, &procedure.result_streams);
    encode_strings(out, &procedure.required_permissions);
    encode_transaction_policy(out, procedure.transaction_policy);
    push_u8(
        out,
        match procedure.compatibility_policy {
            CompatibilityPolicy::AdditiveOnly => 0,
            CompatibilityPolicy::ExactHash => 1,
        },
    );
    push_u8(
        out,
        match procedure.result_metadata_policy {
            ResultMetadataPolicy::RequireBeforePayload => 0,
            ResultMetadataPolicy::AllowStreamingUnknown => 1,
        },
    );
    push_bool(out, procedure.error_policy.rollback_on_error);
    encode_strings(out, &procedure.error_policy.allowed_error_codes);
    push_u8(
        out,
        match procedure.multi_result_policy {
            MultiResultPolicy::SingleResultOnly => 0,
            MultiResultPolicy::MultipleResultStreamsAllowed => 1,
        },
    );
}

fn decode_procedure(decoder: &mut Decoder<'_>) -> AndromedaResult<ProcedureContract> {
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
    let result_streams = decode_result_streams(decoder)?;
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

fn encode_result_streams(out: &mut Vec<u8>, streams: &[ResultStreamContract]) {
    push_u64(out, streams.len() as u64);
    for stream in streams {
        push_u64(out, stream.stream_id);
        encode_string(out, &stream.name);
        encode_columns(out, &stream.columns);
        push_bool(out, stream.row_count_exact_required);
    }
}

fn decode_result_streams(decoder: &mut Decoder<'_>) -> AndromedaResult<Vec<ResultStreamContract>> {
    let count = decoder.len()?;
    let mut streams = Vec::with_capacity(count);
    for _ in 0..count {
        streams.push(ResultStreamContract {
            stream_id: decoder.u64()?,
            name: decoder.string()?,
            columns: decode_columns(decoder)?,
            row_count_exact_required: decoder.bool()?,
        });
    }
    Ok(streams)
}

fn encode_transaction_policy(out: &mut Vec<u8>, policy: TransactionPolicy) {
    push_u8(
        out,
        match policy.access_mode {
            AccessMode::ReadOnly => 0,
            AccessMode::ReadWrite => 1,
        },
    );
    push_u8(
        out,
        match policy.isolation {
            IsolationPolicy::Snapshot => 0,
            IsolationPolicy::Serializable => 1,
        },
    );
    push_bool(out, policy.retryable);
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

fn encode_object_ref(out: &mut Vec<u8>, object: &CatalogObjectRef) {
    push_u64(out, object.object_id.get());
    encode_qualified_name(out, &object.name);
    push_u8(out, object_kind_tag(object.kind));
    push_u64(out, object.catalog_version.get());
}

fn decode_object_ref(decoder: &mut Decoder<'_>) -> AndromedaResult<CatalogObjectRef> {
    Ok(CatalogObjectRef {
        object_id: CatalogObjectId::new(decoder.u64()?),
        name: decode_qualified_name(decoder)?,
        kind: object_kind_from_tag(decoder.u8()?)?,
        catalog_version: CatalogVersion::new(decoder.u64()?),
    })
}

fn object_kind_tag(kind: ObjectKind) -> u8 {
    match kind {
        ObjectKind::Database => 0,
        ObjectKind::Namespace => 1,
        ObjectKind::Table => 2,
        ObjectKind::Map => 3,
        ObjectKind::Enum => 4,
        ObjectKind::StructuredObject => 5,
        ObjectKind::Procedure => 6,
    }
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

fn encode_columns(out: &mut Vec<u8>, columns: &[ColumnDescriptor]) {
    push_u64(out, columns.len() as u64);
    for column in columns {
        encode_string(out, &column.name);
        encode_type_descriptor(out, &column.data_type);
        push_u32(out, column.ordinal);
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

fn encode_type_descriptor(out: &mut Vec<u8>, descriptor: &TypeDescriptor) {
    encode_scalar_type(out, &descriptor.scalar);
    push_u8(
        out,
        match descriptor.absence {
            AbsencePolicy::Required => 0,
            AbsencePolicy::ExplicitOptional => 1,
        },
    );
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

fn encode_scalar_type(out: &mut Vec<u8>, scalar: &ScalarType) {
    match scalar {
        ScalarType::I8 => push_u8(out, 0),
        ScalarType::I16 => push_u8(out, 1),
        ScalarType::I32 => push_u8(out, 2),
        ScalarType::I64 => push_u8(out, 3),
        ScalarType::I128 => push_u8(out, 4),
        ScalarType::U8 => push_u8(out, 5),
        ScalarType::U16 => push_u8(out, 6),
        ScalarType::U32 => push_u8(out, 7),
        ScalarType::U64 => push_u8(out, 8),
        ScalarType::U128 => push_u8(out, 9),
        ScalarType::Decimal(decimal) => {
            push_u8(out, 10);
            encode_decimal_type(out, *decimal);
        }
        ScalarType::Float(float) => {
            push_u8(out, 11);
            encode_float_type(out, *float);
        }
        ScalarType::Bool => push_u8(out, 12),
        ScalarType::Text(text) => {
            push_u8(out, 13);
            push_u8(
                out,
                match text.encoding {
                    TextEncoding::Utf8 => 0,
                    TextEncoding::Utf16 => 1,
                    TextEncoding::Unicode => 2,
                },
            );
            match text.max_length {
                Some(max_length) => {
                    push_bool(out, true);
                    push_u32(out, max_length);
                }
                None => push_bool(out, false),
            }
            match &text.collation {
                Some(collation) => {
                    push_bool(out, true);
                    encode_string(out, collation);
                }
                None => push_bool(out, false),
            }
        }
        ScalarType::Timestamp(timestamp) => {
            push_u8(out, 14);
            push_u8(
                out,
                match timestamp {
                    TimestampType::Transaction => 0,
                    TimestampType::Invocation => 1,
                    TimestampType::MonotonicEpoch => 2,
                },
            );
        }
    }
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

fn encode_decimal_type(out: &mut Vec<u8>, decimal: DecimalType) {
    match decimal {
        DecimalType::Min => push_u8(out, 0),
        DecimalType::Mid => push_u8(out, 1),
        DecimalType::Max => push_u8(out, 2),
        DecimalType::Custom { precision, scale } => {
            push_u8(out, 3);
            push_u8(out, precision);
            push_u8(out, scale);
        }
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

fn encode_float_type(out: &mut Vec<u8>, float: FloatType) {
    match float {
        FloatType::Min => push_u8(out, 0),
        FloatType::Mid => push_u8(out, 1),
        FloatType::Max => push_u8(out, 2),
        FloatType::Custom { bits, mode } => {
            push_u8(out, 3);
            push_u16(out, bits);
            push_u8(
                out,
                match mode {
                    FloatMode::Approximate => 0,
                    FloatMode::DeterministicAnalytics => 1,
                },
            );
        }
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

fn encode_qualified_names(out: &mut Vec<u8>, names: &[QualifiedName]) {
    push_u64(out, names.len() as u64);
    for name in names {
        encode_qualified_name(out, name);
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

fn encode_qualified_name(out: &mut Vec<u8>, name: &QualifiedName) {
    push_u64(out, name.parts().len() as u64);
    for part in name.parts() {
        encode_string(out, part);
    }
}

fn decode_qualified_name(decoder: &mut Decoder<'_>) -> AndromedaResult<QualifiedName> {
    let count = decoder.len()?;
    let mut parts = Vec::with_capacity(count);
    for _ in 0..count {
        parts.push(decoder.string()?);
    }
    QualifiedName::new(parts)
}

fn encode_strings(out: &mut Vec<u8>, strings: &[String]) {
    push_u64(out, strings.len() as u64);
    for value in strings {
        encode_string(out, value);
    }
}

fn decode_strings(decoder: &mut Decoder<'_>) -> AndromedaResult<Vec<String>> {
    let count = decoder.len()?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(decoder.string()?);
    }
    Ok(values)
}

fn encode_string(out: &mut Vec<u8>, value: &str) {
    push_u64(out, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

fn catalog_wal_payload_checksum(kind_tag: u16, body_len: u64, body: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    fn fold_bytes(state: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *state ^= u64::from(*byte);
            *state = state.wrapping_mul(FNV_PRIME);
        }
    }

    let mut state = FNV_OFFSET;
    fold_bytes(&mut state, &CATALOG_WAL_PAYLOAD_MAGIC.to_le_bytes());
    fold_bytes(&mut state, &CATALOG_WAL_PAYLOAD_VERSION_V1.to_le_bytes());
    fold_bytes(&mut state, &kind_tag.to_le_bytes());
    fold_bytes(&mut state, &body_len.to_le_bytes());
    fold_bytes(&mut state, body);
    if state == 0 {
        1
    } else {
        state
    }
}

fn push_bool(out: &mut Vec<u8>, value: bool) {
    push_u8(out, u8::from(value));
}

fn push_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn finish(&self) -> AndromedaResult<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(catalog_error("trailing bytes in catalog WAL payload body"))
        }
    }

    fn bytes(&mut self, len: usize) -> AndromedaResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| catalog_error("catalog WAL payload offset overflow"))?;
        if end > self.bytes.len() {
            return Err(catalog_error("truncated catalog WAL payload body"));
        }
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn u8(&mut self) -> AndromedaResult<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn bool(&mut self) -> AndromedaResult<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(catalog_error("unknown bool tag")),
        }
    }

    fn u16(&mut self) -> AndromedaResult<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> AndromedaResult<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> AndromedaResult<u64> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }

    fn i64(&mut self) -> AndromedaResult<i64> {
        Ok(i64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }

    fn len(&mut self) -> AndromedaResult<usize> {
        usize::try_from(self.u64()?)
            .map_err(|_| catalog_error("catalog WAL collection length does not fit usize"))
    }

    fn string(&mut self) -> AndromedaResult<String> {
        let len = self.len()?;
        let bytes = self.bytes(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| catalog_error("catalog WAL string is not valid UTF-8"))
    }
}

fn catalog_error(message: &str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Catalog, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(id: u64, name: &str, kind: ObjectKind, version: CatalogVersion) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(id),
            name: QualifiedName::parse(name).unwrap(),
            kind,
            catalog_version: version,
        }
    }

    fn table_definition(id: u64, name: &str, version: CatalogVersion) -> CatalogDefinition {
        CatalogDefinition::Table(TableDefinition {
            object: object(id, name, ObjectKind::Table, version),
            columns: vec![ColumnDescriptor {
                name: "ProductId".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }],
        })
    }

    fn boundary() -> CatalogMutationBoundary {
        CatalogMutationBoundary {
            batch_id: crate::DefinitionBatchId::new(7),
            database_id: DatabaseId::new(1),
            namespace_id: NamespaceId::new(2),
            previous_version: CatalogVersion::new(10),
            next_version: CatalogVersion::new(11),
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
        }
    }

    #[test]
    fn durable_payload_round_trips_begin_apply_commit() {
        let definition = table_definition(42, "Inventory.Product", CatalogVersion::new(11));
        let object = definition.object_ref().clone();
        let records = vec![
            CatalogMutationRecord::Begin(boundary()),
            CatalogMutationRecord::Apply(Box::new(CatalogMutationDelta {
                operation_index: 0,
                planned_version: CatalogVersion::new(11),
                operation: CatalogMutationOperation::CreateObject { object, definition },
            })),
            CatalogMutationRecord::Commit(boundary()),
        ];

        for record in records {
            let encoded = record.encode_durable_payload().unwrap();
            let decoded = CatalogMutationRecord::decode_durable_payload(&encoded).unwrap();
            assert_eq!(decoded, record);
        }
    }

    #[test]
    fn durable_payload_kind_tags_match_storage_wal_catalog_taxonomy() {
        assert_eq!(
            CatalogMutationRecordKind::CatalogChangeBegin.storage_wal_kind_tag(),
            19
        );
        assert_eq!(
            CatalogMutationRecordKind::CatalogChangeApply.storage_wal_kind_tag(),
            20
        );
        assert_eq!(
            CatalogMutationRecordKind::CatalogChangeCommit.storage_wal_kind_tag(),
            21
        );
    }

    #[test]
    fn durable_payload_rejects_corruption() {
        let record = CatalogMutationRecord::Begin(boundary());
        let mut encoded = record.encode_durable_payload().unwrap();
        let last = encoded.len() - 1;
        encoded[last] ^= 0x55;

        assert_eq!(
            CatalogMutationRecord::decode_durable_payload(&encoded)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Catalog
        );
    }
}
