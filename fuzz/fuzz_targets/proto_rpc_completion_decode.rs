#![no_main]

use andromeda_proto::{
    decode_generated_message, generated::protocol::v1::RpcCompletion as ProtoRpcCompletion,
};
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_ROW_COUNT_SUMMARIES: usize = 1024;

fuzz_target!(|data: &[u8]| {
    if let Some(completion) =
        common::decode_bounded::<ProtoRpcCompletion, _>(data, common::MAX_64K_INPUT_BYTES, |data| {
            decode_generated_message(data)
        })
    {
        let _ = andromeda_proto::RpcCompletionStatus::from_terminal_code(completion.status as u32);
        let _ = completion
            .result_row_counts
            .iter()
            .take(MAX_ROW_COUNT_SUMMARIES)
            .all(|summary| !summary.result_name.trim().is_empty());
    }
});
