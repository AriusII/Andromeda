use std::collections::BTreeSet;

use andromeda_error::AndromedaResult;

use crate::{CONTRACT_PACKAGE, PROTOCOL_PACKAGE, validate_required_contract_hash};

use super::{
    common::{contract_error, validate_optional_catalog_version},
    result_stream::validate_generated_result_streams,
    views::{
        GeneratedProcedureManifestView, GeneratedProtocolLayoutView,
        GeneratedRequiredPermissionView,
    },
};

pub fn validate_generated_procedure_manifest<T>(manifest: &T) -> AndromedaResult<()>
where
    T: GeneratedProcedureManifestView,
{
    if manifest.procedure_id() == 0 {
        return contract_error("resolved procedure manifest procedure_id must be nonzero");
    }

    if manifest.procedure_name().trim().is_empty() {
        return contract_error("resolved procedure manifest procedure_name must be non-empty");
    }

    validate_required_contract_hash(
        "resolved procedure manifest contract_hash",
        manifest.contract_hash(),
    )?;
    validate_optional_catalog_version(
        "resolved procedure manifest catalog_version",
        Some(manifest.catalog_version()),
    )?;
    validate_required_stats_version(
        "resolved procedure manifest stats_version",
        manifest.stats_version(),
    )?;
    validate_required_contract_hash(
        "resolved procedure manifest policy_version",
        manifest.policy_version(),
    )?;

    let Some(protocol_layout) = manifest.protocol_layout() else {
        return contract_error("resolved procedure manifest requires protocol_layout");
    };
    let contract_hash = manifest.contract_hash();
    let policy_version = manifest.policy_version();
    let descriptor_set_hash = protocol_layout.descriptor_set_hash();
    let frame_envelope_hash = protocol_layout.frame_envelope_hash();
    validate_required_contract_hash(
        "resolved procedure manifest descriptor_set_hash",
        descriptor_set_hash,
    )?;
    validate_required_contract_hash(
        "resolved procedure manifest frame_envelope_hash",
        frame_envelope_hash,
    )?;
    if protocol_layout.protocol_package() != PROTOCOL_PACKAGE {
        return contract_error("resolved procedure manifest protocol_package mismatch");
    }
    if protocol_layout.contract_package() != CONTRACT_PACKAGE {
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

    validate_generated_result_streams(manifest.result_streams())?;
    validate_generated_required_permissions(manifest.required_permissions())?;

    Ok(())
}

fn validate_generated_required_permissions<T>(permissions: &[T]) -> AndromedaResult<()>
where
    T: GeneratedRequiredPermissionView,
{
    if permissions.is_empty() {
        return contract_error(
            "resolved procedure manifest requires at least one required permission",
        );
    }

    let mut seen_permissions = BTreeSet::new();
    for permission in permissions {
        if permission.id().trim().is_empty() {
            return contract_error("resolved procedure manifest permission id must be non-empty");
        }
        if permission.id() != permission.id().to_ascii_lowercase() {
            return contract_error("resolved procedure manifest permission id must be lower-case");
        }
        if permission.family().trim().is_empty() {
            return contract_error(
                "resolved procedure manifest permission family must be non-empty",
            );
        }
        if !seen_permissions.insert(permission.id().to_string()) {
            return contract_error(
                "resolved procedure manifest required permissions must be unique",
            );
        }
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
