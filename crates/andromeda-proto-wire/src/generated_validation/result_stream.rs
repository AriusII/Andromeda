use std::collections::BTreeSet;

use andromeda_error::AndromedaResult;

use super::{
    common::{contract_error, protocol_error},
    views::{GeneratedColumnDescriptorView, GeneratedResultStreamDescriptorView},
};

const MAX_GENERATED_RESULT_STREAMS: usize = 128;
const MAX_GENERATED_RESULT_STREAM_COLUMNS: usize = 256;

pub fn validate_generated_result_streams<T>(descriptors: &[T]) -> AndromedaResult<()>
where
    T: GeneratedResultStreamDescriptorView,
{
    if descriptors.len() > MAX_GENERATED_RESULT_STREAMS {
        return contract_error(format!(
            "resolved procedure manifest result streams exceed bounded limit of {MAX_GENERATED_RESULT_STREAMS}"
        ));
    }

    let mut seen_streams = BTreeSet::new();
    for descriptor in descriptors {
        validate_generated_result_stream_descriptor(descriptor)?;
        if !seen_streams.insert(descriptor.stream_name().to_string()) {
            return contract_error(
                "resolved procedure manifest result stream names must be unique",
            );
        }
    }

    Ok(())
}

fn validate_generated_result_stream_descriptor<T>(descriptor: &T) -> AndromedaResult<()>
where
    T: GeneratedResultStreamDescriptorView,
{
    if descriptor.stream_name().trim().is_empty() {
        return contract_error("resolved result stream descriptor name must be non-empty");
    }

    let cardinality = validate_result_cardinality(descriptor.cardinality())?;
    let row_count_requirement = validate_row_count_requirement(descriptor.row_count_requirement())?;
    validate_generated_columns(descriptor.columns())?;

    if row_count_requirement == RowCountRequirementCode::ExactRequired
        && descriptor.row_count_exact().is_none()
    {
        return contract_error("resolved result stream requires row_count_exact");
    }

    if let Some(row_count_exact) = descriptor.row_count_exact()
        && !cardinality_permits_exact(cardinality, row_count_exact)
    {
        return contract_error("resolved result stream row_count_exact violates cardinality");
    }

    if let Some(row_count_max) = descriptor.row_count_max() {
        if !cardinality_permits_max(cardinality, row_count_max) {
            return contract_error("resolved result stream row_count_max violates cardinality");
        }
        if let Some(row_count_exact) = descriptor.row_count_exact()
            && row_count_exact > row_count_max
        {
            return contract_error("resolved result stream row_count_exact exceeds row_count_max");
        }
    }

    Ok(())
}

fn validate_generated_columns<T>(columns: &[T]) -> AndromedaResult<()>
where
    T: GeneratedColumnDescriptorView,
{
    if columns.is_empty() {
        return contract_error("resolved result stream requires at least one typed column");
    }
    if columns.len() > MAX_GENERATED_RESULT_STREAM_COLUMNS {
        return contract_error(format!(
            "resolved result stream columns exceed bounded limit of {MAX_GENERATED_RESULT_STREAM_COLUMNS}"
        ));
    }

    let mut seen_names = BTreeSet::new();
    let mut seen_ordinals = BTreeSet::new();
    for column in columns {
        if column.name().trim().is_empty() {
            return contract_error("resolved result stream column name must be non-empty");
        }
        if column.type_name().trim().is_empty() {
            return contract_error("resolved result stream column type_name must be non-empty");
        }
        if !seen_names.insert(column.name().to_string()) {
            return contract_error("resolved result stream column names must be unique");
        }
        if !seen_ordinals.insert(column.ordinal()) {
            return contract_error("resolved result stream column ordinals must be unique");
        }
    }

    for expected in 0..columns.len() as u32 {
        if !seen_ordinals.contains(&expected) {
            return contract_error(
                "resolved result stream column ordinals must be dense and zero-based",
            );
        }
    }

    Ok(())
}

fn validate_result_cardinality(cardinality: i32) -> AndromedaResult<ResultCardinalityCode> {
    match cardinality {
        1 => Ok(ResultCardinalityCode::ZeroOrMore),
        2 => Ok(ResultCardinalityCode::ZeroOrOne),
        3 => Ok(ResultCardinalityCode::OneOrMore),
        4 => Ok(ResultCardinalityCode::ExactlyOne),
        0 => contract_error("resolved result stream cardinality must be specified"),
        _ => protocol_error("unknown resolved result stream cardinality"),
    }
}

fn validate_row_count_requirement(requirement: i32) -> AndromedaResult<RowCountRequirementCode> {
    match requirement {
        1 => Ok(RowCountRequirementCode::UnknownAllowed),
        2 => Ok(RowCountRequirementCode::ExactIfKnown),
        3 => Ok(RowCountRequirementCode::ExactRequired),
        0 => contract_error("resolved result stream row_count_requirement must be specified"),
        _ => protocol_error("unknown resolved result stream row_count_requirement"),
    }
}

fn cardinality_permits_exact(cardinality: ResultCardinalityCode, exact: u64) -> bool {
    match cardinality {
        ResultCardinalityCode::ZeroOrMore => true,
        ResultCardinalityCode::ZeroOrOne => exact <= 1,
        ResultCardinalityCode::OneOrMore => exact >= 1,
        ResultCardinalityCode::ExactlyOne => exact == 1,
    }
}

fn cardinality_permits_max(cardinality: ResultCardinalityCode, max: u64) -> bool {
    match cardinality {
        ResultCardinalityCode::ZeroOrMore => true,
        ResultCardinalityCode::ZeroOrOne => max <= 1,
        ResultCardinalityCode::OneOrMore => max >= 1,
        ResultCardinalityCode::ExactlyOne => max == 1,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResultCardinalityCode {
    ZeroOrMore,
    ZeroOrOne,
    OneOrMore,
    ExactlyOne,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowCountRequirementCode {
    UnknownAllowed,
    ExactIfKnown,
    ExactRequired,
}
