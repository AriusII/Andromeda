use andromeda_error::AndromedaResult;
use andromeda_procedure_contract::{AccessMode, ProcedureContract};
use andromeda_types::{
    AbsencePolicy, CatalogVersion, ColumnDescriptor, DecimalType, FloatMode, FloatType, ScalarType,
    TextEncoding, TimestampType, TypeDescriptor,
};

use crate::{ColumnSchema, ProcedureManifest};

pub(super) fn manifest_from_contract(
    contract: &ProcedureContract,
) -> AndromedaResult<ProcedureManifest> {
    contract.validate_canonical_hash()?;
    let output_schema = contract
        .result_streams
        .first()
        .map(|stream| columns_to_schema(&stream.columns))
        .transpose()?
        .unwrap_or_default();
    let manifest = ProcedureManifest {
        procedure_id: contract.procedure_id,
        qualified_name: contract.object.name.as_catalog_path(),
        catalog_version: contract.object.catalog_version,
        contract_hash: contract.contract_hash.as_bytes().to_vec(),
        input_schema: columns_to_schema(&contract.inputs)?,
        output_schema,
        is_mutable: matches!(
            contract.transaction_policy.access_mode,
            AccessMode::ReadWrite
        ),
        min_compatible_version: CatalogVersion::new(1),
    };
    manifest.validate()?;
    Ok(manifest)
}

fn columns_to_schema(columns: &[ColumnDescriptor]) -> AndromedaResult<Vec<ColumnSchema>> {
    columns
        .iter()
        .map(|column| {
            column.validate()?;
            Ok(ColumnSchema {
                name: column.name.clone(),
                type_descriptor: type_descriptor_name(&column.data_type),
                ordinal: column.ordinal,
                nullable: matches!(column.data_type.absence, AbsencePolicy::ExplicitOptional),
            })
        })
        .collect()
}

fn type_descriptor_name(descriptor: &TypeDescriptor) -> String {
    match &descriptor.scalar {
        ScalarType::I8 => "int8".to_string(),
        ScalarType::I16 => "int16".to_string(),
        ScalarType::I32 => "int32".to_string(),
        ScalarType::I64 => "int64".to_string(),
        ScalarType::I128 => "int128".to_string(),
        ScalarType::U8 => "uint8".to_string(),
        ScalarType::U16 => "uint16".to_string(),
        ScalarType::U32 => "uint32".to_string(),
        ScalarType::U64 => "uint64".to_string(),
        ScalarType::U128 => "uint128".to_string(),
        ScalarType::Decimal(decimal) => decimal_type_name(*decimal),
        ScalarType::Float(float) => float_type_name(*float),
        ScalarType::Bool => "bool".to_string(),
        ScalarType::Text(text) => text_type_name(text.encoding),
        ScalarType::Timestamp(timestamp) => timestamp_type_name(*timestamp),
    }
}

fn decimal_type_name(decimal: DecimalType) -> String {
    match decimal {
        DecimalType::Min => "decimal(min)".to_string(),
        DecimalType::Mid => "decimal(mid)".to_string(),
        DecimalType::Max => "decimal(max)".to_string(),
        DecimalType::Custom { precision, scale } => format!("decimal({precision},{scale})"),
    }
}

fn float_type_name(float: FloatType) -> String {
    match float {
        FloatType::Min => "float(min)".to_string(),
        FloatType::Mid => "float(mid)".to_string(),
        FloatType::Max => "float(max)".to_string(),
        FloatType::Custom { bits, mode } => {
            let mode = match mode {
                FloatMode::Approximate => "approx",
                FloatMode::DeterministicAnalytics => "deterministic",
            };
            format!("float({bits},{mode})")
        },
    }
}

fn text_type_name(encoding: TextEncoding) -> String {
    match encoding {
        TextEncoding::Utf8 => "text(utf8)".to_string(),
        TextEncoding::Utf16 => "text(utf16)".to_string(),
        TextEncoding::Unicode => "text(unicode)".to_string(),
    }
}

fn timestamp_type_name(timestamp: TimestampType) -> String {
    match timestamp {
        TimestampType::Transaction => "timestamp(transaction)".to_string(),
        TimestampType::Invocation => "timestamp(invocation)".to_string(),
        TimestampType::MonotonicEpoch => "timestamp(monotonic_epoch)".to_string(),
    }
}
