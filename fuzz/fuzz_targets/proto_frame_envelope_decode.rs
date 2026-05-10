#![no_main]

use andromeda_proto::generated::protocol::v1::FrameEnvelope as ProtoFrameEnvelope;
use andromeda_proto_wire::decode_protobuf_message;
use andromeda_rpc_protocol::{PayloadKind, ProtocolVersion};
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    if let Some(envelope) =
        common::decode_bounded::<ProtoFrameEnvelope, _>(data, common::MAX_64K_INPUT_BYTES, |data| {
            decode_protobuf_message(data, "generated FrameEnvelope")
        })
    {
        let _ = envelope.protocol_version.as_ref().map(|version| {
            let protocol_version = ProtocolVersion {
                major: version.major,
                minor: version.minor,
            };
            protocol_version.validate()
        });
        let _ = PayloadKind::try_from(envelope.payload_kind as u32);
    }
});
