use andromeda_error::AndromedaResult;

use crate::generated::protocol;

pub fn validate_generated_error_envelope(
    error: &protocol::v1::ErrorEnvelope,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_error_envelope(error)
}
