#![no_main]

use andromeda_proto::{
    decode_generated_message, generated::protocol::v1::FrameEnvelope as ProtoFrameEnvelope,
};
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    if let Some(envelope) =
        common::decode_bounded::<ProtoFrameEnvelope, _>(data, common::MAX_64K_INPUT_BYTES, |data| {
            decode_generated_message(data)
        })
    {
        let _ = envelope.protocol_version.as_ref().map(|version| {
            let protocol_version = andromeda_proto::ProtocolVersion {
                major: version.major,
                minor: version.minor,
            };
            protocol_version.validate()
        });
        let _ = andromeda_proto::PayloadKind::try_from(envelope.payload_kind as u32);
    }
});
