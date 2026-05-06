#![forbid(unsafe_code)]

//! Property-based checks for generated `FrameEnvelope` decoding and validation.

use andromeda_core::{CatalogVersion, ContractHash, RequestId, SessionId, TransactionId};
use andromeda_proto::{
    FrameEnvelope, PayloadKind, ProtocolVersion, decode_generated_message,
    encode_generated_message,
    generated::protocol::v1::{
        FrameEnvelope as ProtoFrameEnvelope, PayloadKind as ProtoPayloadKind,
        ProtocolVersion as ProtoProtocolVersion,
    },
};
use proptest::prelude::*;
use std::panic;

fn arb_protobuf_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..4096)
}

fn arb_payload() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 1..1024)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EnvelopeDecision {
    Accept(EnvelopeFacts),
    Reject(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnvelopeFacts {
    protocol_major: u32,
    protocol_minor: u32,
    payload_kind: PayloadKind,
    contract_hash_is_zero: bool,
    payload_len: usize,
}

impl EnvelopeFacts {
    fn assert_valid(&self) {
        assert_eq!(self.protocol_major, ProtocolVersion::SUPPORTED_MAJOR);
        assert_eq!(self.protocol_minor, ProtocolVersion::SUPPORTED_MINOR);

        if self.payload_kind.requires_contract_hash() {
            assert!(
                !self.contract_hash_is_zero,
                "contract-bound payloads require a nonzero contract hash"
            );
        }

        if self.payload_kind.requires_non_empty_payload() {
            assert!(
                self.payload_len > 0,
                "payload kind {:?} requires non-empty payload bytes",
                self.payload_kind
            );
        }
    }
}

fn assert_decision_invariants(decision: &EnvelopeDecision) {
    match decision {
        EnvelopeDecision::Accept(facts) => facts.assert_valid(),
        EnvelopeDecision::Reject(reason) => {
            assert!(!reason.trim().is_empty(), "reject decisions need a reason");
        }
    }
}

fn validate_envelope_safely(data: &[u8]) -> EnvelopeDecision {
    let proto: ProtoFrameEnvelope = match decode_generated_message(data) {
        Ok(proto) => proto,
        Err(error) => return EnvelopeDecision::Reject(error.to_string()),
    };

    validate_proto_envelope(proto)
}

fn validate_proto_envelope(proto: ProtoFrameEnvelope) -> EnvelopeDecision {
    let Some(proto_version) = proto.protocol_version else {
        return EnvelopeDecision::Reject("frame envelope requires protocol_version".to_string());
    };

    let payload_kind = match PayloadKind::try_from(proto.payload_kind as u32) {
        Ok(payload_kind) => payload_kind,
        Err(error) => return EnvelopeDecision::Reject(error.to_string()),
    };

    let contract_hash = if proto.contract_hash.is_empty() {
        ContractHash::zero()
    } else {
        match ContractHash::from_slice(&proto.contract_hash) {
            Ok(contract_hash) => contract_hash,
            Err(error) => return EnvelopeDecision::Reject(error.to_string()),
        }
    };

    let envelope = FrameEnvelope {
        protocol_version: ProtocolVersion {
            major: proto_version.major,
            minor: proto_version.minor,
        },
        contract_hash,
        catalog_version: CatalogVersion::new(proto.catalog_version),
        request_id: RequestId::new(proto.request_id),
        session_id: SessionId::new(proto.session_id),
        tx_id: proto.tx_id.map(TransactionId::new),
        payload_kind,
        payload: proto.payload,
    };

    match envelope.validate() {
        Ok(()) => EnvelopeDecision::Accept(EnvelopeFacts {
            protocol_major: envelope.protocol_version.major,
            protocol_minor: envelope.protocol_version.minor,
            payload_kind: envelope.payload_kind,
            contract_hash_is_zero: envelope.contract_hash.is_zero(),
            payload_len: envelope.payload.len(),
        }),
        Err(error) => EnvelopeDecision::Reject(error.to_string()),
    }
}

fn valid_proto_envelope(
    payload_kind: ProtoPayloadKind,
    payload: impl Into<Vec<u8>>,
) -> ProtoFrameEnvelope {
    ProtoFrameEnvelope {
        protocol_version: Some(ProtoProtocolVersion { major: 1, minor: 0 }),
        contract_hash: vec![0xA5; ContractHash::LEN],
        catalog_version: 1,
        request_id: 10,
        session_id: 20,
        tx_id: Some(30),
        payload_kind: payload_kind as i32,
        payload: payload.into(),
    }
}

fn encoded_valid_rpc_execute(payload: Vec<u8>) -> Vec<u8> {
    encode_generated_message(&valid_proto_envelope(
        ProtoPayloadKind::RpcExecuteRequest,
        payload,
    ))
}

#[test]
fn prop_envelope_validation_never_panics() {
    proptest!(|(data in arb_protobuf_bytes())| {
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            validate_envelope_safely(&data)
        }));

        prop_assert!(result.is_ok(), "envelope validation panicked");
        assert_decision_invariants(&result.unwrap());
    });
}

#[test]
fn prop_envelope_validation_classifies_arbitrary_bytes() {
    proptest!(|(data in arb_protobuf_bytes())| {
        let decision = validate_envelope_safely(&data);
        assert_decision_invariants(&decision);
    });
}

#[test]
fn empty_bytes_are_rejected() {
    let decision = validate_envelope_safely(&[]);

    assert!(matches!(decision, EnvelopeDecision::Reject(_)));
    assert_decision_invariants(&decision);
}

#[test]
fn prop_valid_rpc_execute_envelopes_are_accepted() {
    proptest!(|(payload in arb_payload())| {
        let decision = validate_envelope_safely(&encoded_valid_rpc_execute(payload.clone()));

        match decision {
            EnvelopeDecision::Accept(facts) => {
                prop_assert_eq!(facts.payload_kind, PayloadKind::RpcExecuteRequest);
                prop_assert_eq!(facts.payload_len, payload.len());
                prop_assert!(!facts.contract_hash_is_zero);
            }
            EnvelopeDecision::Reject(reason) => {
                prop_assert!(false, "valid RPC execute envelope rejected: {reason}");
            }
        }
    });
}

#[test]
fn prop_valid_generated_envelopes_round_trip_before_validation() {
    proptest!(|(payload in arb_payload())| {
        let proto = valid_proto_envelope(ProtoPayloadKind::RpcBatch, payload);
        let bytes = encode_generated_message(&proto);
        let decoded: ProtoFrameEnvelope =
            decode_generated_message(&bytes).expect("valid generated envelope decodes");

        prop_assert_eq!(&decoded, &proto);
        let decision = validate_proto_envelope(decoded);
        assert_decision_invariants(&decision);
        prop_assert!(matches!(decision, EnvelopeDecision::Accept(_)));
    });
}

#[test]
fn prop_truncated_valid_rpc_execute_envelopes_are_rejected() {
    proptest!(|(payload in arb_payload())| {
        let bytes = encoded_valid_rpc_execute(payload);
        prop_assume!(bytes.len() > 1);

        let truncated = &bytes[..bytes.len() - 1];
        let decision = validate_envelope_safely(truncated);

        prop_assert!(matches!(decision, EnvelopeDecision::Reject(_)));
        assert_decision_invariants(&decision);
    });
}

#[test]
fn prop_unsupported_protocol_versions_are_rejected() {
    proptest!(|(
        major in 0u32..4,
        minor in 0u32..4,
        payload in arb_payload(),
    )| {
        prop_assume!(major != ProtocolVersion::SUPPORTED_MAJOR || minor > ProtocolVersion::SUPPORTED_MINOR);

        let mut proto = valid_proto_envelope(ProtoPayloadKind::RpcExecuteRequest, payload);
        proto.protocol_version = Some(ProtoProtocolVersion { major, minor });
        let decision = validate_proto_envelope(proto);

        prop_assert!(matches!(decision, EnvelopeDecision::Reject(_)));
        assert_decision_invariants(&decision);
    });
}

#[test]
fn prop_contract_bound_payloads_reject_invalid_hash_lengths() {
    proptest!(|(
        hash_bytes in prop::collection::vec(any::<u8>(), 0..64),
        payload in arb_payload(),
    )| {
        prop_assume!(hash_bytes.len() != ContractHash::LEN);

        let mut proto = valid_proto_envelope(ProtoPayloadKind::RpcExecuteRequest, payload);
        proto.contract_hash = hash_bytes;
        let decision = validate_proto_envelope(proto);

        prop_assert!(matches!(decision, EnvelopeDecision::Reject(_)));
        assert_decision_invariants(&decision);
    });
}

#[test]
fn contract_bound_payloads_reject_zero_hash() {
    let mut proto = valid_proto_envelope(ProtoPayloadKind::RpcCompletion, Vec::new());
    proto.contract_hash = vec![0; ContractHash::LEN];

    let decision = validate_proto_envelope(proto);

    assert!(matches!(decision, EnvelopeDecision::Reject(_)));
    assert_decision_invariants(&decision);
}

#[test]
fn rpc_batch_requires_non_empty_payload() {
    let proto = valid_proto_envelope(ProtoPayloadKind::RpcBatch, Vec::new());
    let decision = validate_proto_envelope(proto);

    assert!(matches!(decision, EnvelopeDecision::Reject(_)));
    assert_decision_invariants(&decision);
}

#[test]
fn rpc_completion_accepts_empty_payload_with_contract_binding() {
    let proto = valid_proto_envelope(ProtoPayloadKind::RpcCompletion, Vec::new());
    let decision = validate_proto_envelope(proto);

    assert!(matches!(
        decision,
        EnvelopeDecision::Accept(EnvelopeFacts {
            payload_kind: PayloadKind::RpcCompletion,
            payload_len: 0,
            ..
        })
    ));
    assert_decision_invariants(&decision);
}

#[test]
fn unknown_payload_kind_is_rejected() {
    let mut proto = valid_proto_envelope(ProtoPayloadKind::RpcCompletion, Vec::new());
    proto.payload_kind = 999;
    let decision = validate_proto_envelope(proto);

    assert!(matches!(decision, EnvelopeDecision::Reject(_)));
    assert_decision_invariants(&decision);
}

#[test]
fn prop_envelope_validation_is_deterministic() {
    proptest!(|(data in arb_protobuf_bytes())| {
        let decision1 = validate_envelope_safely(&data);
        let decision2 = validate_envelope_safely(&data);

        prop_assert_eq!(&decision1, &decision2);
        assert_decision_invariants(&decision1);
    });
}

#[test]
fn integration_envelope_processing_pipeline_counts_every_decision() {
    proptest!(|(envelopes in prop::collection::vec(arb_protobuf_bytes(), 1..64))| {
        let mut accepted = 0usize;
        let mut rejected = 0usize;

        for envelope in &envelopes {
            let decision = validate_envelope_safely(envelope);
            assert_decision_invariants(&decision);
            match decision {
                EnvelopeDecision::Accept(_) => accepted += 1,
                EnvelopeDecision::Reject(_) => rejected += 1,
            }
        }

        prop_assert_eq!(accepted + rejected, envelopes.len());
    });
}

#[test]
fn edge_case_single_byte_is_rejected_with_reason() {
    let decision = validate_envelope_safely(&[0xFF]);

    assert!(matches!(decision, EnvelopeDecision::Reject(_)));
    assert_decision_invariants(&decision);
}

#[test]
fn edge_case_all_zeros_is_rejected_with_reason() {
    let decision = validate_envelope_safely(&[0x00; 1000]);

    assert!(matches!(decision, EnvelopeDecision::Reject(_)));
    assert_decision_invariants(&decision);
}

#[test]
fn edge_case_all_ones_is_rejected_with_reason() {
    let decision = validate_envelope_safely(&[0xFF; 1000]);

    assert!(matches!(decision, EnvelopeDecision::Reject(_)));
    assert_decision_invariants(&decision);
}
