use andromeda_error::AndromedaResult;

use crate::generated::protocol;

pub fn validate_generated_rpc_completion(
    completion: &protocol::v1::RpcCompletion,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_rpc_completion(completion)
}
