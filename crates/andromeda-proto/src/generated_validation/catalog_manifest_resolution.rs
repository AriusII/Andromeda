use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::generated::contract;

use super::{
    validate_generated_procedure_manifest, validate_generated_protocol_version,
    validate_optional_catalog_version, validate_optional_contract_hash,
};

pub(crate) fn validate_catalog_procedure_manifest_resolution_request(
    request: &contract::v1::CatalogProcedureManifestResolutionRequest,
) -> AndromedaResult<()> {
    validate_generated_protocol_version(request.protocol_major, request.protocol_minor)?;

    if request.request_id == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "catalog manifest resolution request_id must be nonzero",
        ));
    }

    match &request.selector {
        Some(contract::v1::catalog_procedure_manifest_resolution_request::Selector::ProcedureId(
            procedure_id,
        )) if *procedure_id != 0 => {}
        Some(contract::v1::catalog_procedure_manifest_resolution_request::Selector::ProcedureName(
            procedure_name,
        )) if !procedure_name.trim().is_empty() => {}
        Some(_) => {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "catalog manifest resolution selector must be nonzero/non-empty",
            ));
        }
        None => {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "catalog manifest resolution request requires a selector",
            ));
        }
    }

    validate_optional_contract_hash(
        "catalog manifest resolution expected_contract_hash",
        request.expected_contract_hash.as_deref(),
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution expected_catalog_version",
        request.expected_catalog_version,
    )?;

    Ok(())
}

pub(crate) fn validate_catalog_procedure_manifest_resolution_response(
    response: &contract::v1::CatalogProcedureManifestResolutionResponse,
) -> AndromedaResult<()> {
    validate_generated_protocol_version(response.protocol_major, response.protocol_minor)?;

    if response.request_id == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "catalog manifest resolution response request_id must be nonzero",
        ));
    }

    validate_resolution_status(response.status)?;
    validate_optional_contract_hash(
        "catalog manifest resolution resolved_contract_hash",
        response.resolved_contract_hash.as_deref(),
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution resolved_catalog_version",
        response.resolved_catalog_version,
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution current_catalog_version",
        response.current_catalog_version,
    )?;

    if response.status == 1 {
        let manifest = response.manifest.as_ref().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "resolved catalog manifest response requires a manifest",
            )
        })?;
        validate_generated_procedure_manifest(manifest)?;

        if response.resolved_contract_hash.as_deref() != Some(manifest.contract_hash.as_slice()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "resolved contract hash must match manifest contract_hash",
            ));
        }

        if response.resolved_catalog_version != Some(manifest.catalog_version) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "resolved catalog version must match manifest catalog_version",
            ));
        }
    } else if response.manifest.is_some() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "non-resolved catalog manifest response must not carry a manifest",
        ));
    }

    Ok(())
}

fn validate_resolution_status(status: i32) -> AndromedaResult<()> {
    match status {
        1..=11 => Ok(()),
        0 => Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "catalog manifest resolution status must be specified",
        )),
        _ => Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "unknown catalog manifest resolution status",
        )),
    }
}
