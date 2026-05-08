#![no_main]

use andromeda_proto::{
    decode_generated_message,
    generated::protocol::v1::InvocationResponse as ProtoInvocationResponse,
    validate_generated_invocation_response, validate_generated_invocation_response_sequence,
};
use libfuzzer_sys::fuzz_target;

mod common;

const LENGTH_PREFIXED_SEQUENCE_FORMAT: u8 = 1;
const MAX_SEQUENCE_RESPONSES: usize = 128;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, common::MAX_64K_INPUT_BYTES);

    if let Some(response) = common::decode_bounded::<ProtoInvocationResponse, _>(
        data,
        common::MAX_64K_INPUT_BYTES,
        |data| decode_generated_message(data),
    ) {
        let _ = validate_generated_invocation_response(&response);
        let single = [response];
        let _ = validate_generated_invocation_response_sequence(&single);
    }

    if let Some(sequence) = decode_length_prefixed_sequence(data) {
        for response in &sequence {
            let _ = validate_generated_invocation_response(response);
        }
        let _ = validate_generated_invocation_response_sequence(&sequence);
    }
});

fn decode_length_prefixed_sequence(data: &[u8]) -> Option<Vec<ProtoInvocationResponse>> {
    if data.first().copied()? != LENGTH_PREFIXED_SEQUENCE_FORMAT {
        return None;
    }

    let mut offset = 1;
    let mut sequence = Vec::new();
    while offset + 2 <= data.len() && sequence.len() < MAX_SEQUENCE_RESPONSES {
        let frame_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        let next = offset.checked_add(frame_len)?;
        let frame = data.get(offset..next)?;
        let response = decode_generated_message(frame).ok()?;
        sequence.push(response);
        offset = next;
    }

    (!sequence.is_empty()).then_some(sequence)
}
