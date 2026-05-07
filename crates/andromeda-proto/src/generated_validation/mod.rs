mod catalog_manifest_resolution;
mod completion;
mod hash;
mod manifest;
mod protocol_version;
mod runtime_projection;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub use catalog_manifest_resolution::{
    validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response,
};
pub use completion::validate_generated_rpc_completion;
pub(crate) use hash::{validate_optional_contract_hash, validate_required_contract_hash};
pub(crate) use manifest::{
    validate_generated_procedure_manifest, validate_generated_result_streams,
    validate_optional_catalog_version,
};
pub(crate) use protocol_version::validate_generated_protocol_version;
pub use runtime_projection::{
    project_generated_frame_envelope, validate_generated_error_envelope,
    validate_generated_frame_envelope, validate_generated_invocation_request,
    validate_generated_invocation_response, validate_generated_invocation_response_sequence,
    validate_generated_rpc_batch, validate_generated_rpc_execute_request,
    validate_generated_rpc_metadata,
};

pub(crate) fn contract_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Contract,
        message.into(),
    ))
}

pub(crate) fn protocol_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Protocol,
        message.into(),
    ))
}
