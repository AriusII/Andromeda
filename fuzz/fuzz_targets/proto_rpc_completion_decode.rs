#![no_main]

use andromeda_proto::{
    decode_generated_message, generated::protocol::v1::RpcCompletion as ProtoRpcCompletion,
};
use libfuzzer_sys::fuzz_target;

const MAX_PROTO_FUZZ_BYTES: usize = 64 * 1024;
const MAX_ROW_COUNT_SUMMARIES: usize = 1024;

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(MAX_PROTO_FUZZ_BYTES)];
    let decoded: andromeda_core::AndromedaResult<ProtoRpcCompletion> =
        decode_generated_message(data);

    if let Ok(completion) = decoded {
        let _ = andromeda_proto::RpcCompletionStatus::from_terminal_code(completion.status as u32);
        let _ = completion
            .result_row_counts
            .iter()
            .take(MAX_ROW_COUNT_SUMMARIES)
            .all(|summary| !summary.result_name.trim().is_empty());
    }
});
