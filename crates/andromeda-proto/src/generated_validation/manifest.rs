use andromeda_core::AndromedaResult;

use crate::generated::{CONTRACT_PACKAGE, PROTOCOL_PACKAGE, contract};

use super::{contract_error, validate_required_contract_hash};

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
    validate_required_contract_hash(
        "resolved procedure manifest policy_version",
        &manifest.policy_version,
    )?;

    let Some(protocol_layout) = manifest.protocol_layout.as_ref() else {
        return contract_error("resolved procedure manifest requires protocol_layout");
    };
    validate_required_contract_hash(
        "resolved procedure manifest descriptor_set_hash",
        &protocol_layout.descriptor_set_hash,
    )?;
    validate_required_contract_hash(
        "resolved procedure manifest frame_envelope_hash",
        &protocol_layout.frame_envelope_hash,
    )?;
    if protocol_layout.protocol_package != PROTOCOL_PACKAGE {
        return contract_error("resolved procedure manifest protocol_package mismatch");
    }
    if protocol_layout.contract_package != CONTRACT_PACKAGE {
        return contract_error("resolved procedure manifest contract_package mismatch");
    }

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
