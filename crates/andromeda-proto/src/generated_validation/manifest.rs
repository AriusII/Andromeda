use std::collections::BTreeSet;

use andromeda_core::AndromedaResult;

use crate::generated::contract::v1::result_stream_descriptor::{Cardinality, RowCountRequirement};
use crate::generated::{CONTRACT_PACKAGE, PROTOCOL_PACKAGE, contract};

use super::{contract_error, validate_required_contract_hash};

const MAX_GENERATED_RESULT_STREAMS: usize = 128;
const MAX_GENERATED_RESULT_STREAM_COLUMNS: usize = 256;

pub(crate) fn validate_generated_procedure_manifest(
    manifest: &contract::v1::ProcedureManifest,
) -> AndromedaResult<()> {
    if manifest.procedure_id == 0 {
        return contract_error("resolved procedure manifest procedure_id must be nonzero");
    }

    if manifest.procedure_name.trim().is_empty() {
        return contract_error("resolved procedure manifest procedure_name must be non-empty");
    }

    validate_required_contract_hash(
        "resolved procedure manifest contract_hash",
        &manifest.contract_hash,
    )?;
    validate_optional_catalog_version(
        "resolved procedure manifest catalog_version",
        Some(manifest.catalog_version),
    )?;
    validate_required_stats_version(
        "resolved procedure manifest stats_version",
        manifest.stats_version,
    )?;
    validate_required_contract_hash(
        "resolved procedure manifest policy_version",
        &manifest.policy_version,
    )?;

    let Some(protocol_layout) = manifest.protocol_layout.as_ref() else {
        return contract_error("resolved procedure manifest requires protocol_layout");
    };
    let contract_hash = manifest.contract_hash.as_slice();
    let policy_version = manifest.policy_version.as_slice();
    let descriptor_set_hash = protocol_layout.descriptor_set_hash.as_slice();
    let frame_envelope_hash = protocol_layout.frame_envelope_hash.as_slice();
    validate_required_contract_hash(
        "resolved procedure manifest descriptor_set_hash",
        descriptor_set_hash,
    )?;
    validate_required_contract_hash(
        "resolved procedure manifest frame_envelope_hash",
        frame_envelope_hash,
    )?;
    if protocol_layout.protocol_package != PROTOCOL_PACKAGE {
        return contract_error("resolved procedure manifest protocol_package mismatch");
    }
    if protocol_layout.contract_package != CONTRACT_PACKAGE {
        return contract_error("resolved procedure manifest contract_package mismatch");
    }
    if descriptor_set_hash == frame_envelope_hash {
        return contract_error(
            "resolved procedure manifest descriptor and frame envelope hashes must be distinct",
        );
    }
    if contract_hash == descriptor_set_hash || contract_hash == frame_envelope_hash {
        return contract_error(
            "resolved procedure manifest contract_hash must be distinct from protocol layout hashes",
        );
    }
    if policy_version == contract_hash
        || policy_version == descriptor_set_hash
        || policy_version == frame_envelope_hash
    {
        return contract_error(
            "resolved procedure manifest policy_version must be distinct from contract and protocol layout hashes",
        );
    }

    validate_generated_result_streams(&manifest.result_streams)?;
    validate_generated_required_permissions(&manifest.required_permissions)?;

    Ok(())
}

pub(crate) fn validate_optional_catalog_version(
    label: &str,
    version: Option<u64>,
) -> AndromedaResult<()> {
    if version == Some(0) {
        return contract_error(format!("{label} must be nonzero when present"));
    }

    Ok(())
}

fn validate_required_stats_version(label: &str, version: Option<u64>) -> AndromedaResult<()> {
    match version {
        Some(value) if value != 0 => Ok(()),
        Some(_) => contract_error(format!("{label} must be nonzero")),
        None => contract_error(format!("{label} must be present")),
    }
}

pub(crate) fn validate_generated_result_streams(
    descriptors: &[contract::v1::ResultStreamDescriptor],
) -> AndromedaResult<()> {
    if descriptors.len() > MAX_GENERATED_RESULT_STREAMS {
        return contract_error(format!(
            "resolved procedure manifest result streams exceed bounded limit of {MAX_GENERATED_RESULT_STREAMS}"
        ));
    }

    let mut seen_streams = BTreeSet::new();
    for descriptor in descriptors {
        validate_generated_result_stream_descriptor(descriptor)?;
        if !seen_streams.insert(descriptor.stream_name.clone()) {
            return contract_error(
                "resolved procedure manifest result stream names must be unique",
            );
        }
    }

    Ok(())
}

fn validate_generated_result_stream_descriptor(
    descriptor: &contract::v1::ResultStreamDescriptor,
) -> AndromedaResult<()> {
    if descriptor.stream_name.trim().is_empty() {
        return contract_error("resolved result stream descriptor name must be non-empty");
    }

    let cardinality = validate_result_cardinality(descriptor.cardinality)?;
    let row_count_requirement = validate_row_count_requirement(descriptor.row_count_requirement)?;
    validate_generated_columns(&descriptor.columns)?;

    if row_count_requirement == RowCountRequirement::ExactRequired
        && descriptor.row_count_exact.is_none()
    {
        return contract_error("resolved result stream requires row_count_exact");
    }

    if let Some(row_count_exact) = descriptor.row_count_exact
        && !cardinality_permits_exact(cardinality, row_count_exact)
    {
        return contract_error("resolved result stream row_count_exact violates cardinality");
    }

    if let Some(row_count_max) = descriptor.row_count_max {
        if !cardinality_permits_max(cardinality, row_count_max) {
            return contract_error("resolved result stream row_count_max violates cardinality");
        }
        if let Some(row_count_exact) = descriptor.row_count_exact
            && row_count_exact > row_count_max
        {
            return contract_error("resolved result stream row_count_exact exceeds row_count_max");
        }
    }

    Ok(())
}

fn validate_generated_columns(columns: &[contract::v1::ColumnDescriptor]) -> AndromedaResult<()> {
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
        if column.name.trim().is_empty() {
            return contract_error("resolved result stream column name must be non-empty");
        }
        if column.type_name.trim().is_empty() {
            return contract_error("resolved result stream column type_name must be non-empty");
        }
        if !seen_names.insert(column.name.clone()) {
            return contract_error("resolved result stream column names must be unique");
        }
        if !seen_ordinals.insert(column.ordinal) {
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

fn validate_generated_required_permissions(
    permissions: &[contract::v1::RequiredPermission],
) -> AndromedaResult<()> {
    if permissions.is_empty() {
        return contract_error(
            "resolved procedure manifest requires at least one required permission",
        );
    }

    let mut seen_permissions = BTreeSet::new();
    for permission in permissions {
        if permission.id.trim().is_empty() {
            return contract_error("resolved procedure manifest permission id must be non-empty");
        }
        if permission.id != permission.id.to_ascii_lowercase() {
            return contract_error("resolved procedure manifest permission id must be lower-case");
        }
        if permission.family.trim().is_empty() {
            return contract_error(
                "resolved procedure manifest permission family must be non-empty",
            );
        }
        if !seen_permissions.insert(permission.id.clone()) {
            return contract_error(
                "resolved procedure manifest required permissions must be unique",
            );
        }
    }

    Ok(())
}

fn validate_result_cardinality(cardinality: i32) -> AndromedaResult<Cardinality> {
    match cardinality {
        value if value == Cardinality::ZeroOrMore as i32 => Ok(Cardinality::ZeroOrMore),
        value if value == Cardinality::ZeroOrOne as i32 => Ok(Cardinality::ZeroOrOne),
        value if value == Cardinality::OneOrMore as i32 => Ok(Cardinality::OneOrMore),
        value if value == Cardinality::ExactlyOne as i32 => Ok(Cardinality::ExactlyOne),
        value if value == Cardinality::Unspecified as i32 => {
            contract_error("resolved result stream cardinality must be specified")
        }
        _ => super::protocol_error("unknown resolved result stream cardinality"),
    }
}

fn validate_row_count_requirement(requirement: i32) -> AndromedaResult<RowCountRequirement> {
    match requirement {
        value if value == RowCountRequirement::UnknownAllowed as i32 => {
            Ok(RowCountRequirement::UnknownAllowed)
        }
        value if value == RowCountRequirement::ExactIfKnown as i32 => {
            Ok(RowCountRequirement::ExactIfKnown)
        }
        value if value == RowCountRequirement::ExactRequired as i32 => {
            Ok(RowCountRequirement::ExactRequired)
        }
        value if value == RowCountRequirement::Unspecified as i32 => {
            contract_error("resolved result stream row_count_requirement must be specified")
        }
        _ => super::protocol_error("unknown resolved result stream row_count_requirement"),
    }
}

fn cardinality_permits_exact(cardinality: Cardinality, exact: u64) -> bool {
    match cardinality {
        Cardinality::ZeroOrMore => true,
        Cardinality::ZeroOrOne => exact <= 1,
        Cardinality::OneOrMore => exact >= 1,
        Cardinality::ExactlyOne => exact == 1,
        Cardinality::Unspecified => false,
    }
}

fn cardinality_permits_max(cardinality: Cardinality, max: u64) -> bool {
    match cardinality {
        Cardinality::ZeroOrMore => true,
        Cardinality::ZeroOrOne => max <= 1,
        Cardinality::OneOrMore => max >= 1,
        Cardinality::ExactlyOne => max == 1,
        Cardinality::Unspecified => false,
    }
}
