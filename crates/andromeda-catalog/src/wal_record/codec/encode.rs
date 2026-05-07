//! Body encoding for catalog mutation WAL payloads.

use andromeda_core::{
    AbsencePolicy, AndromedaResult, ColumnDescriptor, DecimalType, FloatMode, FloatType,
    ScalarType, TextEncoding, TimestampType, TypeDescriptor,
};

use crate::{
    AccessMode, CatalogDefinition, CatalogLifecycleTarget, CatalogMutationBoundary,
    CatalogMutationDelta, CatalogMutationOperation, CatalogMutationRecord, CatalogObjectRef,
    CatalogPublicationSemantics, CompatibilityPolicy, EnumDefinition, IsolationPolicy,
    MultiResultPolicy, ObjectKind, ProcedureContract, QualifiedName, ResultMetadataPolicy,
    ResultStreamContract, StructuredObjectDefinition, TableDefinition, TransactionPolicy,
};

use super::super::constants::{
    CATALOG_WAL_PAYLOAD_HEADER_LEN, CATALOG_WAL_PAYLOAD_VERSION_CURRENT,
};
use super::{
    catalog_error, catalog_wal_payload_checksum,
    fields::{push_bool, push_i64, push_u8, push_u16, push_u32, push_u64},
    format::encode_payload_header,
};

pub(super) fn encode_durable_payload(record: &CatalogMutationRecord) -> AndromedaResult<Vec<u8>> {
    record.validate_for_durable_payload()?;

    let mut body = Vec::new();
    match record {
        CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
            encode_boundary(&mut body, boundary)
        }
        CatalogMutationRecord::Apply(delta) => encode_delta(&mut body, delta),
    }

    let kind_tag = record.kind().storage_wal_kind_tag();
    let body_len = u64::try_from(body.len())
        .map_err(|_| catalog_error("catalog WAL payload body is too large"))?;
    let checksum = catalog_wal_payload_checksum(
        CATALOG_WAL_PAYLOAD_VERSION_CURRENT,
        kind_tag,
        body_len,
        &body,
    );

    let mut encoded = Vec::with_capacity(CATALOG_WAL_PAYLOAD_HEADER_LEN + body.len());
    encode_payload_header(&mut encoded, kind_tag, body_len, checksum);
    encoded.extend_from_slice(&body);
    Ok(encoded)
}

fn encode_boundary(out: &mut Vec<u8>, boundary: &CatalogMutationBoundary) {
    push_u64(out, boundary.batch_id.get());
    push_u64(out, boundary.database_id.get());
    push_u64(out, boundary.namespace_id.get());
    push_u64(out, boundary.previous_version.get());
    push_u64(out, boundary.next_version.get());
    out.extend_from_slice(&boundary.source_hash.as_bytes());
    out.extend_from_slice(&boundary.dependency_graph_hash.as_bytes());
    push_u64(out, boundary.expected_apply_count as u64);
    push_u8(
        out,
        match boundary.publication_semantics {
            CatalogPublicationSemantics::PlannedVersionOnly => 0,
            CatalogPublicationSemantics::DurablePublicationExternal => 1,
        },
    );
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

fn encode_lifecycle_target(out: &mut Vec<u8>, target: &CatalogLifecycleTarget) {
    encode_object_ref(out, &target.object);
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

fn encode_table(out: &mut Vec<u8>, table: &TableDefinition) {
    encode_object_ref(out, &table.object);
    encode_columns(out, &table.columns);
}

fn encode_structured_object(out: &mut Vec<u8>, object: &StructuredObjectDefinition) {
    encode_object_ref(out, &object.object);
    encode_columns(out, &object.fields);
    encode_strings(out, &object.unique_by);
}

fn encode_enum(out: &mut Vec<u8>, definition: &EnumDefinition) {
    encode_object_ref(out, &definition.object);
    push_u64(out, definition.variants.len() as u64);
    for variant in &definition.variants {
        encode_string(out, &variant.name);
        push_i64(out, variant.value);
    }
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

fn encode_result_streams(out: &mut Vec<u8>, streams: &[ResultStreamContract]) {
    push_u64(out, streams.len() as u64);
    for stream in streams {
        push_u64(out, stream.stream_id);
        encode_string(out, &stream.name);
        encode_columns(out, &stream.columns);
        push_bool(out, stream.row_count_exact_required);
        push_u8(out, stream.cardinality.stable_tag());
    }
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

fn encode_object_ref(out: &mut Vec<u8>, object: &CatalogObjectRef) {
    push_u64(out, object.object_id.get());
    encode_qualified_name(out, &object.name);
    push_u8(out, object_kind_tag(object.kind));
    push_u64(out, object.catalog_version.get());
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

fn encode_columns(out: &mut Vec<u8>, columns: &[ColumnDescriptor]) {
    push_u64(out, columns.len() as u64);
    for column in columns {
        encode_string(out, &column.name);
        encode_type_descriptor(out, &column.data_type);
        push_u32(out, column.ordinal);
    }
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

fn encode_qualified_names(out: &mut Vec<u8>, names: &[QualifiedName]) {
    push_u64(out, names.len() as u64);
    for name in names {
        encode_qualified_name(out, name);
    }
}

fn encode_qualified_name(out: &mut Vec<u8>, name: &QualifiedName) {
    push_u64(out, name.parts().len() as u64);
    for part in name.parts() {
        encode_string(out, part);
    }
}

fn encode_strings(out: &mut Vec<u8>, strings: &[String]) {
    push_u64(out, strings.len() as u64);
    for value in strings {
        encode_string(out, value);
    }
}

fn encode_string(out: &mut Vec<u8>, value: &str) {
    push_u64(out, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}
