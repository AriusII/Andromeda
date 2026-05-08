use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::generated::contract;

pub(crate) fn validate_generated_procedure_manifest(
    manifest: &contract::v1::ProcedureManifest,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_procedure_manifest(manifest)
}

pub(crate) fn validate_generated_result_streams(
    descriptors: &[contract::v1::ResultStreamDescriptor],
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_result_streams(descriptors)
}

pub(crate) fn validate_optional_catalog_version(
    label: &str,
    version: Option<u64>,
) -> AndromedaResult<()> {
    if version == Some(0) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!("{label} must be nonzero when present"),
        ));
    }

    Ok(())
}
