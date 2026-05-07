use prost::Message;

use andromeda_proto::generated::andromeda::protocol::v1::FrameEnvelope;

#[test]
fn test_no_codec_panics_on_malformed_input() {
    let malformed_inputs = [
        vec![],
        vec![0xFF, 0xFF, 0xFF, 0xFF],
        vec![0x08, 0xFF, 0xFF, 0xFF, 0xFF],
    ];

    for malformed in malformed_inputs {
        let result = FrameEnvelope::decode(malformed.as_slice());
        let _ = result;
    }
}
