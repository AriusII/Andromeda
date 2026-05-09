use andromeda_error::AndromedaResult;

use crate::validate_optional_contract_hash;

use super::{
    common::{contract_error, protocol_error, validate_optional_catalog_version},
    frame_envelope::validate_generated_protocol_version,
    procedure_manifest::validate_generated_procedure_manifest,
    views::{
        GeneratedCatalogManifestResolutionRequestView,
        GeneratedCatalogManifestResolutionResponseView, GeneratedCatalogManifestResolutionSelector,
        GeneratedProcedureManifestView,
    },
};

pub fn validate_catalog_procedure_manifest_resolution_request<T>(request: &T) -> AndromedaResult<()>
where
    T: GeneratedCatalogManifestResolutionRequestView,
{
    validate_generated_protocol_version(request.protocol_major(), request.protocol_minor())?;

    if request.request_id() == 0 {
        return protocol_error("catalog manifest resolution request_id must be nonzero");
    }

    match request.selector() {
        Some(GeneratedCatalogManifestResolutionSelector::ProcedureId(procedure_id))
            if procedure_id != 0 => {},
        Some(GeneratedCatalogManifestResolutionSelector::ProcedureName(procedure_name))
            if !procedure_name.trim().is_empty() => {},
        Some(_) => {
            return contract_error(
                "catalog manifest resolution selector must be nonzero/non-empty",
            );
        },
        None => {
            return contract_error("catalog manifest resolution request requires a selector");
        },
    }

    validate_optional_contract_hash(
        "catalog manifest resolution expected_contract_hash",
        request.expected_contract_hash(),
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution expected_catalog_version",
        request.expected_catalog_version(),
    )?;

    Ok(())
}

pub fn validate_catalog_procedure_manifest_resolution_response<T>(
    response: &T,
) -> AndromedaResult<()>
where
    T: GeneratedCatalogManifestResolutionResponseView,
{
    validate_generated_protocol_version(response.protocol_major(), response.protocol_minor())?;

    if response.request_id() == 0 {
        return protocol_error("catalog manifest resolution response request_id must be nonzero");
    }

    let status = validate_resolution_status(response.status())?;
    validate_optional_contract_hash(
        "catalog manifest resolution resolved_contract_hash",
        response.resolved_contract_hash(),
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution resolved_catalog_version",
        response.resolved_catalog_version(),
    )?;
    validate_optional_catalog_version(
        "catalog manifest resolution current_catalog_version",
        response.current_catalog_version(),
    )?;

    if status == ResolutionStatusCode::Resolved {
        let Some(manifest) = response.manifest() else {
            return contract_error("resolved catalog manifest response requires a manifest");
        };
        validate_generated_procedure_manifest(manifest)?;

        if response.resolved_contract_hash() != Some(manifest.contract_hash()) {
            return contract_error("resolved contract hash must match manifest contract_hash");
        }

        if response.resolved_catalog_version() != Some(manifest.catalog_version()) {
            return contract_error("resolved catalog version must match manifest catalog_version");
        }
    } else {
        validate_unresolved_response_fields(response)?;
    }

    Ok(())
}

fn validate_unresolved_response_fields<T>(response: &T) -> AndromedaResult<()>
where
    T: GeneratedCatalogManifestResolutionResponseView,
{
    if response.manifest().is_some() {
        return contract_error("non-resolved catalog manifest response must not carry a manifest");
    }

    if response.resolved_contract_hash().is_some() {
        return contract_error(
            "non-resolved catalog manifest response must not carry resolved_contract_hash",
        );
    }

    if response.resolved_catalog_version().is_some() {
        return contract_error(
            "non-resolved catalog manifest response must not carry resolved_catalog_version",
        );
    }

    Ok(())
}

fn validate_resolution_status(status: i32) -> AndromedaResult<ResolutionStatusCode> {
    match status {
        1 => Ok(ResolutionStatusCode::Resolved),
        2 => Ok(ResolutionStatusCode::NotFound),
        3 => Ok(ResolutionStatusCode::CatalogVersionMismatch),
        4 => Ok(ResolutionStatusCode::ContractHashMismatch),
        5 => Ok(ResolutionStatusCode::NotSourceGeneratorReady),
        6 => Ok(ResolutionStatusCode::PermissionDenied),
        7 => Ok(ResolutionStatusCode::Unsupported),
        8 => Ok(ResolutionStatusCode::Malformed),
        9 => Ok(ResolutionStatusCode::Internal),
        10 => Ok(ResolutionStatusCode::CatalogNotReady),
        11 => Ok(ResolutionStatusCode::AuthRequired),
        0 => protocol_error("catalog manifest resolution status must be specified"),
        _ => protocol_error("unknown catalog manifest resolution status"),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResolutionStatusCode {
    Resolved,
    NotFound,
    CatalogVersionMismatch,
    ContractHashMismatch,
    NotSourceGeneratorReady,
    PermissionDenied,
    Unsupported,
    Malformed,
    Internal,
    CatalogNotReady,
    AuthRequired,
}
