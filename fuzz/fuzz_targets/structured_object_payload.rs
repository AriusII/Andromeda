#![no_main]

use andromeda_proto::generated::contract::v1::StructuredObjectHeader as ProtoHeader;
use andromeda_proto_wire::{
    decode_protobuf_message, project_generated_structured_object_header,
    validate_generated_structured_object_header,
};
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_STRUCTURED_OBJECT_INPUT_BYTES: usize = 64 * 1024;
const LENGTH_PREFIX_LEN: usize = 2;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, MAX_STRUCTURED_OBJECT_INPUT_BYTES);

    if let Some(header) =
        common::decode_bounded::<ProtoHeader, _>(data, MAX_STRUCTURED_OBJECT_INPUT_BYTES, |bytes| {
            decode_protobuf_message(bytes, "generated StructuredObjectHeader")
        })
    {
        let _ = validate_generated_structured_object_header(&header);
        let _ = project_generated_structured_object_header(&header);
    }

    if let Some((header_bytes, payload)) = split_length_prefixed_header(data) {
        if let Ok(header) =
            decode_protobuf_message::<ProtoHeader>(header_bytes, "generated StructuredObjectHeader")
        {
            let _ = validate_generated_structured_object_header(&header);
            if let Ok(typed) = project_generated_structured_object_header(&header) {
                // TODO(structured-object-payload-codec): Replace this local
                // length-prefixed harness envelope with the owner payload decoder
                // once StructuredObject payload bytes have a promoted codec. Until
                // then, this target exercises generated header decode/projection
                // and metadata-before-payload length coherence only.
                if let Ok(actual_payload_len) = u64::try_from(payload.len()) {
                    let _payload_length_matches = typed.payload_length == actual_payload_len;
                }
            }
        }
    }
});

fn split_length_prefixed_header(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let len_bytes = data.get(..LENGTH_PREFIX_LEN)?;
    let header_len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;
    let header_start = LENGTH_PREFIX_LEN;
    let header_end = header_start.checked_add(header_len)?;
    let header = data.get(header_start..header_end)?;
    let payload = data.get(header_end..)?;
    Some((header, payload))
}
