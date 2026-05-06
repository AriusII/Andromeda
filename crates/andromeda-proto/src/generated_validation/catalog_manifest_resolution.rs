use andromeda_core::AndromedaResult;

use crate::generated::contract;
use crate::generated::contract::v1::catalog_procedure_manifest_resolution_response::Status as ResolutionStatus;

use super::{
    contract_error, protocol_error, validate_generated_procedure_manifest,
    validate_generated_protocol_version, validate_optional_catalog_version,
    validate_optional_contract_hash,
};

pub fn validate_catalog_procedure_manifest_resolution_request(
    request: &contract::v1::CatalogProcedureManifestResolutionRequest,
) -> AndromedaResult<()> {
    validate_generated_protocol_version(request.protocol_major, request.protocol_minor)?;

    if request.request_id == 0 {
        return protocol_error("catalog manifest resolution request_id must be nonzero");
    }

    match &request.selector {
        Some(
            contract::v1::catalog_procedure_manifest_resolution_request::Selector::ProcedureId(
                procedure_id,
            ),
        ) if *procedure_id != 0 => {}
        Some(
            contract::v1::catalog_procedure_manifest_resolution_request::Selector::ProcedureName(
                procedure_name,
            ),
        ) if !procedure_name.trim().is_empty() => {}
        Some(_) => {
            return contract_error(
                "catalog manifest resolution selector must be nonzero/non-empty",
            );
        }
        None => {
            return contract_error("catalog manifest resolution request requires a selector");
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

pub fn validate_catalog_procedure_manifest_resolution_response(
    response: &contract::v1::CatalogProcedureManifestResolutionResponse,
) -> AndromedaResult<()> {
    validate_generated_protocol_version(response.protocol_major, response.protocol_minor)?;

    if response.request_id == 0 {
        return protocol_error("catalog manifest resolution response request_id must be nonzero");
    }

    let status = validate_resolution_status(response.status)?;
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

    if status == ResolutionStatus::Resolved {
        let Some(manifest) = response.manifest.as_ref() else {
            return contract_error("resolved catalog manifest response requires a manifest");
        };
        validate_generated_procedure_manifest(manifest)?;

        if response.resolved_contract_hash.as_deref() != Some(manifest.contract_hash.as_slice()) {
            return contract_error("resolved contract hash must match manifest contract_hash");
        }

        if response.resolved_catalog_version != Some(manifest.catalog_version) {
            return contract_error("resolved catalog version must match manifest catalog_version");
        }
    } else {
        validate_unresolved_response_fields(response)?;
    }

    Ok(())
}

fn validate_unresolved_response_fields(
    response: &contract::v1::CatalogProcedureManifestResolutionResponse,
) -> AndromedaResult<()> {
    if response.manifest.is_some() {
        return contract_error("non-resolved catalog manifest response must not carry a manifest");
    }

    if response.resolved_contract_hash.is_some() {
        return contract_error(
            "non-resolved catalog manifest response must not carry resolved_contract_hash",
        );
    }

    if response.resolved_catalog_version.is_some() {
        return contract_error(
            "non-resolved catalog manifest response must not carry resolved_catalog_version",
        );
    }

    Ok(())
}

fn validate_resolution_status(status: i32) -> AndromedaResult<ResolutionStatus> {
    match status {
        value if value == ResolutionStatus::Resolved as i32 => Ok(ResolutionStatus::Resolved),
        value if value == ResolutionStatus::NotFound as i32 => Ok(ResolutionStatus::NotFound),
        value if value == ResolutionStatus::CatalogVersionMismatch as i32 => {
            Ok(ResolutionStatus::CatalogVersionMismatch)
        }
        value if value == ResolutionStatus::ContractHashMismatch as i32 => {
            Ok(ResolutionStatus::ContractHashMismatch)
        }
        value if value == ResolutionStatus::NotSourceGeneratorReady as i32 => {
            Ok(ResolutionStatus::NotSourceGeneratorReady)
        }
        value if value == ResolutionStatus::PermissionDenied as i32 => {
            Ok(ResolutionStatus::PermissionDenied)
        }
        value if value == ResolutionStatus::Unsupported as i32 => Ok(ResolutionStatus::Unsupported),
        value if value == ResolutionStatus::Malformed as i32 => Ok(ResolutionStatus::Malformed),
        value if value == ResolutionStatus::Internal as i32 => Ok(ResolutionStatus::Internal),
        value if value == ResolutionStatus::CatalogNotReady as i32 => {
            Ok(ResolutionStatus::CatalogNotReady)
        }
        value if value == ResolutionStatus::AuthRequired as i32 => {
            Ok(ResolutionStatus::AuthRequired)
        }
        value if value == ResolutionStatus::Unspecified as i32 => {
            protocol_error("catalog manifest resolution status must be specified")
        }
        _ => protocol_error("unknown catalog manifest resolution status"),
    }
}
