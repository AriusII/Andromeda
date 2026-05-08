//! Protobuf wire projections for RPC frame envelopes.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
pub use andromeda_rpc_protocol::{
    AUTH_WIRE_CODE, CONTRACT_REQUEST_WIRE_CODE, CONTRACT_RESPONSE_WIRE_CODE, ERROR_WIRE_CODE,
    FrameEnvelope, HELLO_WIRE_CODE, PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP, PayloadFrameFamily,
    PayloadFrameMapping, PayloadKind, ProtocolVersion, RPC_BATCH_WIRE_CODE,
    RPC_COMPLETION_WIRE_CODE, RPC_EXECUTE_REQUEST_WIRE_CODE, RPC_METADATA_WIRE_CODE,
    RpcResultStreamMetadataPolicy,
};
use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId, TransactionId};

use crate::generated::{GeneratedFrameEnvelope, GeneratedProtocolVersion};
pub const fn project_generated_protocol_version(
    version: &GeneratedProtocolVersion,
) -> ProtocolVersion {
    ProtocolVersion {
        major: version.major,
        minor: version.minor,
    }
}

pub fn project_generated_payload_kind(payload_kind: i32) -> AndromedaResult<PayloadKind> {
    let Ok(code) = u32::try_from(payload_kind) else {
        return protocol_error("unknown generated frame envelope payload_kind");
    };

    PayloadKind::try_from(code)
}

pub fn project_generated_frame_envelope(
    envelope: GeneratedFrameEnvelope,
) -> AndromedaResult<FrameEnvelope> {
    let Some(version) = envelope.protocol_version.as_ref() else {
        return protocol_error("generated frame envelope requires protocol_version");
    };

    FrameEnvelope {
        protocol_version: project_generated_protocol_version(version),
        contract_hash: ContractHash::from_slice(&envelope.contract_hash)?,
        catalog_version: CatalogVersion::new(envelope.catalog_version),
        request_id: RequestId::new(envelope.request_id),
        session_id: SessionId::new(envelope.session_id),
        tx_id: envelope.tx_id.map(TransactionId::new),
        payload_kind: project_generated_payload_kind(envelope.payload_kind)?,
        payload: envelope.payload,
    }
    .validated()
}

fn protocol_error<T>(message: &'static str) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Protocol, message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GeneratedPayloadKind;

    #[test]
    fn generated_frame_envelope_projects_to_protocol_owner() {
        let envelope = project_generated_frame_envelope(GeneratedFrameEnvelope {
            protocol_version: Some(GeneratedProtocolVersion { major: 1, minor: 0 }),
            contract_hash: ContractHash::test_vector(7).as_bytes().to_vec(),
            catalog_version: 3,
            request_id: 10,
            session_id: 20,
            tx_id: Some(30),
            payload_kind: GeneratedPayloadKind::RpcExecuteRequest as i32,
            payload: b"ProductId=42;Quantity=3".to_vec(),
        })
        .unwrap();

        assert_eq!(envelope.protocol_version, ProtocolVersion::V1);
        assert_eq!(envelope.payload_kind, PayloadKind::RpcExecuteRequest);
        assert_eq!(envelope.contract_hash, ContractHash::test_vector(7));
        assert_eq!(envelope.catalog_version, CatalogVersion::new(3));
        assert_eq!(envelope.request_id, RequestId::new(10));
        assert_eq!(envelope.session_id, SessionId::new(20));
        assert_eq!(envelope.tx_id, Some(TransactionId::new(30)));
    }

    #[test]
    fn generated_payload_kind_projection_rejects_unspecified() {
        assert_eq!(
            project_generated_payload_kind(GeneratedPayloadKind::Unspecified as i32)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
