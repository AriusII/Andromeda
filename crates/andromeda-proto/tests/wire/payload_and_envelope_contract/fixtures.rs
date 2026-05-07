use andromeda_proto::{FrameEnvelope, PayloadKind, ProtocolVersion};
use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId, TransactionId};

pub(crate) fn hash(byte: u8) -> ContractHash {
    ContractHash::test_vector(byte)
}

pub(crate) fn envelope(payload_kind: PayloadKind, payload: impl Into<Vec<u8>>) -> FrameEnvelope {
    FrameEnvelope {
        protocol_version: ProtocolVersion::V1,
        contract_hash: hash(7),
        catalog_version: CatalogVersion::new(11),
        request_id: RequestId::new(101),
        session_id: SessionId::new(202),
        tx_id: Some(TransactionId::new(303)),
        payload_kind,
        payload: payload.into(),
    }
}
