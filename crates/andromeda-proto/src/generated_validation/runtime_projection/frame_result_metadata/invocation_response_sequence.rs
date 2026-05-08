use andromeda_error::AndromedaResult;

use crate::generated::protocol;

pub fn validate_generated_invocation_response_sequence(
    responses: &[protocol::v1::InvocationResponse],
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_invocation_response_sequence(responses)
}
