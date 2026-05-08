use andromeda_error::AndromedaResult;

use crate::generated::protocol;

pub fn validate_generated_rpc_execute_request(
    request: &protocol::v1::RpcExecuteRequest,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_rpc_execute_request(request)
}

pub fn validate_generated_invocation_request(
    request: &protocol::v1::InvocationRequest,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_invocation_request(request)
}
