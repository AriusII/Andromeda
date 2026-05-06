#![no_main]

use andromeda_proto::{
    decode_generated_message, generated::protocol::v1::RpcCompletion as ProtoRpcCompletion,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let decoded: andromeda_core::AndromedaResult<ProtoRpcCompletion> =
        decode_generated_message(data);

    if let Ok(completion) = decoded {
        let _ = andromeda_proto::RpcCompletionStatus::from_terminal_code(completion.status as u32);
        let _ = completion
            .result_row_counts
            .iter()
            .all(|summary| !summary.result_name.trim().is_empty());
    }
});
