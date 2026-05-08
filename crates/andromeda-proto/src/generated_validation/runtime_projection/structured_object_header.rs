use std::collections::BTreeSet;

use andromeda_error::AndromedaResult;
use andromeda_types::{ColumnDescriptor, ContractHash, ScalarType, TypeDescriptor};

use crate::generated::contract;
use crate::{RowCountPolicy, StructuredObjectHeader, StructuredObjectLayout};

use super::super::{contract_error, protocol_error, validate_required_contract_hash};

const MAX_GENERATED_STRUCTURED_OBJECT_FIELDS: usize = 256;
const MAX_GENERATED_STRUCTURED_OBJECT_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;

pub fn project_generated_structured_object_header(
    header: &contract::v1::StructuredObjectHeader,
) -> AndromedaResult<StructuredObjectHeader> {
    validate_required_contract_hash(
        "generated StructuredObjectHeader contract_hash",
        &header.contract_hash,
    )?;
    validate_required_contract_hash(
        "generated StructuredObjectHeader descriptor_hash",
        &header.descriptor_hash,
    )?;
    validate_generated_structured_object_payload_limits(header)?;

    let fields = project_generated_structured_object_fields(&header.fields)?;
    let typed = StructuredObjectHeader {
        name: header.name.clone(),
        contract_hash: ContractHash::from_slice(&header.contract_hash)?,
        descriptor_hash: ContractHash::from_slice(&header.descriptor_hash)?,
        fields,
        column_count: header.column_count,
        layout: project_generated_structured_object_layout(header.layout)?,
        row_count_policy: project_generated_structured_object_row_count_policy(
            header.row_count_policy,
        )?,
        row_count_exact: project_generated_structured_object_row_count_exact(
            header.row_count_exact,
        )?,
        payload_length: header.payload_length,
        payload_checksum: header.payload_checksum,
        max_payload_length: header.max_payload_length,
    };

    typed.validate()?;
    Ok(typed)
}

pub fn validate_generated_structured_object_header(
    header: &contract::v1::StructuredObjectHeader,
) -> AndromedaResult<()> {
    project_generated_structured_object_header(header).map(|_| ())
}

fn project_generated_structured_object_fields(
    fields: &[contract::v1::ColumnDescriptor],
) -> AndromedaResult<Vec<ColumnDescriptor>> {
    if fields.is_empty() {
        return contract_error("generated StructuredObjectHeader requires at least one field");
    }
    if fields.len() > MAX_GENERATED_STRUCTURED_OBJECT_FIELDS {
        return contract_error(format!(
            "generated StructuredObjectHeader fields exceed bounded limit of {MAX_GENERATED_STRUCTURED_OBJECT_FIELDS}"
        ));
    }

    let mut seen_names = BTreeSet::new();
    let mut seen_ordinals = BTreeSet::new();
    let mut projected = Vec::with_capacity(fields.len());
    for field in fields {
        if field.name.trim().is_empty() {
            return contract_error("generated StructuredObjectHeader field name must be non-empty");
        }
        if !seen_names.insert(field.name.clone()) {
            return contract_error("generated StructuredObjectHeader field names must be unique");
        }
        if !seen_ordinals.insert(field.ordinal) {
            return contract_error(
                "generated StructuredObjectHeader field ordinals must be unique",
            );
        }
        projected.push(ColumnDescriptor {
            name: field.name.clone(),
            data_type: project_generated_structured_object_type(&field.type_name)?,
            ordinal: field.ordinal,
        });
    }

    for expected in 0..fields.len() as u32 {
        if !seen_ordinals.contains(&expected) {
            return contract_error(
                "generated StructuredObjectHeader field ordinals must be dense and zero-based",
            );
        }
    }

    Ok(projected)
}

fn project_generated_structured_object_type(type_name: &str) -> AndromedaResult<TypeDescriptor> {
    if type_name.trim().is_empty() {
        return contract_error(
            "generated StructuredObjectHeader field type_name must be non-empty",
        );
    }

    let scalar = match type_name {
        "i8" => ScalarType::I8,
        "i16" => ScalarType::I16,
        "i32" => ScalarType::I32,
        "i64" => ScalarType::I64,
        "i128" => ScalarType::I128,
        "u8" => ScalarType::U8,
        "u16" => ScalarType::U16,
        "u32" => ScalarType::U32,
        "u64" => ScalarType::U64,
        "u128" => ScalarType::U128,
        "bool" => ScalarType::Bool,
        _ => {
            return contract_error(format!(
                "generated StructuredObjectHeader field type_name '{type_name}' is not in the minimal contract-safe projection set"
            ));
        }
    };

    Ok(TypeDescriptor::required(scalar))
}

fn project_generated_structured_object_layout(
    layout: i32,
) -> AndromedaResult<StructuredObjectLayout> {
    use contract::v1::structured_object_header::Layout;

    match layout {
        value if value == Layout::RowMajor as i32 => Ok(StructuredObjectLayout::RowMajor),
        value if value == Layout::ColumnMajor as i32 => Ok(StructuredObjectLayout::ColumnMajor),
        value if value == Layout::Hybrid as i32 => Ok(StructuredObjectLayout::Hybrid),
        value if value == Layout::Unspecified as i32 => {
            contract_error("generated StructuredObjectHeader layout must be specified")
        }
        _ => protocol_error("unknown generated StructuredObjectHeader layout"),
    }
}

fn project_generated_structured_object_row_count_policy(
    policy: i32,
) -> AndromedaResult<RowCountPolicy> {
    use contract::v1::result_stream_descriptor::RowCountRequirement;

    match policy {
        value if value == RowCountRequirement::UnknownAllowed as i32 => {
            Ok(RowCountPolicy::UnknownAllowed)
        }
        value if value == RowCountRequirement::ExactIfKnown as i32 => {
            Ok(RowCountPolicy::ExactIfKnown)
        }
        value if value == RowCountRequirement::ExactRequired as i32 => {
            Ok(RowCountPolicy::ExactRequired)
        }
        value if value == RowCountRequirement::Unspecified as i32 => {
            contract_error("generated StructuredObjectHeader row_count_policy must be specified")
        }
        _ => protocol_error("unknown generated StructuredObjectHeader row_count_policy"),
    }
}

// Blocker: contract.proto currently exposes row_count_exact as a proto3 scalar,
// so generated code cannot distinguish absent from explicit zero. Keep this
// projection fail-closed until the schema has explicit presence semantics.
fn project_generated_structured_object_row_count_exact(
    row_count_exact: u64,
) -> AndromedaResult<Option<u64>> {
    if row_count_exact == 0 {
        return contract_error(
            "generated StructuredObjectHeader row_count_exact zero is ambiguous until the protobuf field has explicit presence; projection fails closed",
        );
    }

    Ok(Some(row_count_exact))
}

fn validate_generated_structured_object_payload_limits(
    header: &contract::v1::StructuredObjectHeader,
) -> AndromedaResult<()> {
    if header.payload_length > MAX_GENERATED_STRUCTURED_OBJECT_PAYLOAD_BYTES {
        return protocol_error(format!(
            "generated StructuredObjectHeader payload_length exceeds bounded limit of {MAX_GENERATED_STRUCTURED_OBJECT_PAYLOAD_BYTES}"
        ));
    }

    if let Some(max_payload_length) = header.max_payload_length
        && max_payload_length > MAX_GENERATED_STRUCTURED_OBJECT_PAYLOAD_BYTES
    {
        return contract_error(format!(
            "generated StructuredObjectHeader max_payload_length exceeds bounded limit of {MAX_GENERATED_STRUCTURED_OBJECT_PAYLOAD_BYTES}"
        ));
    }

    Ok(())
}
