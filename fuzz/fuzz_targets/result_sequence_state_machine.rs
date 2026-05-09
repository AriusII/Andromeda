#![no_main]

use andromeda_core::{RequestId, SessionId, TransactionId};
use andromeda_rpc_protocol::{
    FrameBytes, FrameHeader, FrameType, ResultStreamMetadataPolicy, ResultStreamSequence,
};
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_SEQUENCE_FUZZ_BYTES: usize = 4 * 1024;
const MAX_SEQUENCE_FRAMES: usize = 128;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, MAX_SEQUENCE_FUZZ_BYTES);
    let policy = match data.first().copied().unwrap_or_default() % 3 {
        0 => ResultStreamMetadataPolicy::RowBatchRequired,
        1 => ResultStreamMetadataPolicy::ZeroRowCompletionAllowed,
        _ => ResultStreamMetadataPolicy::MutationOnly,
    };
    let mut sequence = ResultStreamSequence::new_with_metadata_policy(policy);

    for (index, chunk) in data.chunks(12).take(MAX_SEQUENCE_FRAMES).enumerate() {
        let Some(selector) = chunk.first().copied() else {
            continue;
        };
        let frame_type = match selector % 5 {
            0 => FrameType::RpcMetadata,
            1 => FrameType::RpcBatch,
            2 => FrameType::RpcCompletion,
            3 => FrameType::RpcExecuteRequest,
            _ => FrameType::Error,
        };
        let payload = chunk.get(1..).unwrap_or_default().to_vec();
        let context_seed = if selector & 0x40 == 0 {
            1
        } else {
            index as u64 + 1
        };
        let tx_id = (selector & 0x80 != 0).then(|| TransactionId::new(context_seed));
        let frame = FrameBytes {
            header: FrameHeader {
                frame_type,
                request_id: RequestId::new(context_seed),
                session_id: SessionId::new(context_seed + 1),
                tx_id,
                payload_length: payload.len() as u64,
                flags: 0,
                header_crc: 0,
            },
            payload,
        };

        let _ = sequence.accept(&frame);
    }

    let _ = sequence.is_complete();
});
