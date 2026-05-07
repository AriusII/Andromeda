use proptest::prelude::*;
use prost::Message;

use andromeda_proto::generated::andromeda::protocol::v1::{
    FrameEnvelope, ProtocolVersion, RpcBatch,
};

use crate::proto_wire_fixtures::{
    HASH_LEN, TEST_CONTRACT_HASH_BYTE, generated_protocol_v1 as protocol_v1,
};

proptest! {
    #[test]
    fn prop_protocol_version_roundtrip(major in 0u32..=1000, minor in 0u32..=1000) {
        let version = ProtocolVersion { major, minor };
        let bytes = version.encode_to_vec();
        let decoded = ProtocolVersion::decode(bytes.as_slice())
            .expect("Decode should succeed");

        prop_assert_eq!(version, decoded);
    }

    #[test]
    fn prop_frame_envelope_determinism(
        catalog_version in 0u64..=u64::MAX,
        request_id in 0u64..=u64::MAX,
        session_id in 0u64..=u64::MAX,
    ) {
        let envelope = FrameEnvelope {
            protocol_version: Some(protocol_v1()),
            contract_hash: vec![TEST_CONTRACT_HASH_BYTE; HASH_LEN],
            catalog_version,
            request_id,
            session_id,
            tx_id: None,
            payload_kind: 5,
            payload: vec![1, 2, 3],
        };

        let bytes1 = envelope.encode_to_vec();
        let bytes2 = envelope.encode_to_vec();

        prop_assert_eq!(bytes1, bytes2, "Must serialize deterministically");
    }

    #[test]
    fn prop_rpc_batch_roundtrip(
        batch_index in 0u64..=1000,
        rows_emitted in 0u64..=1_000_000,
    ) {
        let batch = RpcBatch {
            result_name: "result".to_string(),
            batch_index,
            rows_emitted,
            structured_payload: vec![],
            row_count_exact: None,
            terminal_batch: false,
        };

        let bytes = batch.encode_to_vec();
        let decoded = RpcBatch::decode(bytes.as_slice())
            .expect("Decode should succeed");

        prop_assert_eq!(batch, decoded);
    }
}
