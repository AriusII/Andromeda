use andromeda_error::AndromedaResult;

use crate::generated::contract;

pub fn validate_catalog_procedure_manifest_resolution_request(
    request: &contract::v1::CatalogProcedureManifestResolutionRequest,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_catalog_procedure_manifest_resolution_request(request)
}

pub fn validate_catalog_procedure_manifest_resolution_response(
    response: &contract::v1::CatalogProcedureManifestResolutionResponse,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_catalog_procedure_manifest_resolution_response(response)
}
