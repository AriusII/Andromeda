#![no_main]

use andromeda_proto::{
    decode_generated_message, generated::protocol::v1::FrameEnvelope as ProtoFrameEnvelope,
};
use libfuzzer_sys::fuzz_target;

const MAX_PROTO_FUZZ_BYTES: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(MAX_PROTO_FUZZ_BYTES)];
    let decoded: andromeda_core::AndromedaResult<ProtoFrameEnvelope> =
        decode_generated_message(data);

    if let Ok(envelope) = decoded {
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
