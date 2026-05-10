#![no_main]

use andromeda_proto::generated::protocol::v1::RpcExecuteRequest as ProtoRpcExecuteRequest;
use andromeda_proto_wire::{decode_protobuf_message, validate_generated_rpc_execute_request};
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_ARGUMENTS: usize = 128;

fuzz_target!(|data: &[u8]| {
    if let Some(request) = common::decode_bounded::<ProtoRpcExecuteRequest, _>(
        data,
        common::MAX_64K_INPUT_BYTES,
        |data| decode_protobuf_message(data, "generated RpcExecuteRequest"),
    ) {
        let _ = validate_generated_rpc_execute_request(&request);
        let _ = request
            .arguments
            .iter()
            .take(MAX_ARGUMENTS)
            .all(|argument| {
                !argument.name.trim().is_empty() && !argument.type_name.trim().is_empty()
            });
        let _ = request.budget.as_ref().map(|budget| {
            (
                budget.cpu_micros.unwrap_or_default(),
                budget.memory_bytes.unwrap_or_default(),
                budget.io_bytes.unwrap_or_default(),
                budget.priority_class.unwrap_or_default(),
            )
        });
    }
});
