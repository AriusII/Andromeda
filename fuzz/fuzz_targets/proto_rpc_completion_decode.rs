#![no_main]

use andromeda_procedure_contract::RpcCompletionStatus;
use andromeda_proto::generated::protocol::v1::RpcCompletion as ProtoRpcCompletion;
use andromeda_proto_wire::decode_protobuf_message;
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_ROW_COUNT_SUMMARIES: usize = 1024;

fuzz_target!(|data: &[u8]| {
    if let Some(completion) =
        common::decode_bounded::<ProtoRpcCompletion, _>(data, common::MAX_64K_INPUT_BYTES, |data| {
            decode_protobuf_message(data, "generated RpcCompletion")
        })
    {
        let _ = RpcCompletionStatus::from_terminal_code(completion.status as u32);
        let _ = completion
            .result_row_counts
            .iter()
            .take(MAX_ROW_COUNT_SUMMARIES)
            .all(|summary| !summary.result_name.trim().is_empty());
    }
});
