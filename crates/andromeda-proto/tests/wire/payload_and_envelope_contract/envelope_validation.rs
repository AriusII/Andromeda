use andromeda_error::AndromedaErrorKind;
use andromeda_proto::{FrameEnvelope, PayloadKind};
use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId};

use super::fixtures::{envelope, hash};

#[test]
fn contract_bound_envelopes_require_nonzero_contract_hash() {
    for kind in [
        PayloadKind::RpcExecuteRequest,
        PayloadKind::RpcMetadata,
        PayloadKind::RpcBatch,
        PayloadKind::RpcCompletion,
    ] {
        assert!(kind.requires_contract_hash());

        let payload = if kind.requires_non_empty_payload() {
            b"payload".to_vec()
        } else {
            Vec::new()
        };
        let invalid = FrameEnvelope {
            contract_hash: ContractHash::zero(),
            payload_kind: kind,
            payload,
            ..envelope(kind, Vec::new())
        };

        assert_eq!(
            invalid.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    for kind in [
        PayloadKind::Hello,
        PayloadKind::Auth,
        PayloadKind::ContractRequest,
        PayloadKind::ContractResponse,
    ] {
        let contractless = FrameEnvelope {
            contract_hash: ContractHash::zero(),
            payload_kind: kind,
            payload: Vec::new(),
            ..envelope(kind, Vec::new())
        };

        assert!(!kind.requires_contract_hash());
        assert!(
            contractless.validate().is_ok(),
            "{kind:?} should not require a contract hash"
        );
    }

    let typed_error = FrameEnvelope {
        contract_hash: ContractHash::zero(),
        payload_kind: PayloadKind::Error,
        payload: b"typed-error-envelope".to_vec(),
        ..envelope(PayloadKind::Error, Vec::new())
    };
    assert!(!PayloadKind::Error.requires_contract_hash());
    assert!(typed_error.validate().is_ok());

    let empty_error = FrameEnvelope {
        payload: Vec::new(),
        ..typed_error
    };
    assert_eq!(
        empty_error.validate().unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn rpc_execute_and_batch_payload_bodies_are_required() {
    let execute = FrameEnvelope::rpc_execute_request(
        hash(9),
        CatalogVersion::new(12),
        RequestId::new(404),
        SessionId::new(505),
        None,
        Vec::new(),
    );
    assert_eq!(execute.unwrap_err().kind(), AndromedaErrorKind::Protocol);

    let batch = envelope(PayloadKind::RpcBatch, Vec::new());
    assert_eq!(
        batch.validate().unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    assert!(
        envelope(PayloadKind::RpcMetadata, Vec::new())
            .validate()
            .is_ok()
    );
    assert!(
        envelope(PayloadKind::RpcCompletion, Vec::new())
            .validate()
            .is_ok()
    );
}
