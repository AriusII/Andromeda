use prost::Message;

use andromeda_procedure_contract::RpcCompletionStatus;
use andromeda_proto::generated::andromeda::protocol::v1::{RpcCompletion, rpc_completion::Status};
use andromeda_proto_wire::validate_generated_rpc_completion;

fn valid_generated_completion() -> RpcCompletion {
    RpcCompletion {
        status: 1,
        rows_affected: Some(50),
        tx_id: Some(99),
        request_id: Some(10),
        session_id: Some(20),
        trace_id: Some("trace-001".to_string()),
        transaction_outcome: 2,
        durable_lsn: Some(5000),
        result_row_counts: vec![],
    }
}

#[test]
fn test_rpc_completion_all_fields() {
    let completion = valid_generated_completion();

    let bytes = completion.encode_to_vec();
    let decoded = RpcCompletion::decode(bytes.as_slice()).expect("Deserialize completion");

    assert_eq!(completion, decoded);
}

#[test]
fn rpc_completion_status_policy_rejects_unspecified_and_unknown_wire_codes() {
    assert_eq!(Status::Unspecified as i32, 0);
    assert!(RpcCompletionStatus::from_terminal_code(Status::Unspecified as u32).is_none());

    for status in [
        Status::Committed,
        Status::RolledBack,
        Status::FailedBeforeTransaction,
        Status::Cancelled,
        Status::Poisoned,
        Status::PermissionDenied,
        Status::ContractRejected,
        Status::SystemUnavailable,
    ] {
        assert!(
            RpcCompletionStatus::from_terminal_code(status as u32).is_some(),
            "{status:?} must remain an accepted governed completion status"
        );
    }

    for unknown_code in [9_u32, 12, u32::MAX] {
        let completion = RpcCompletion {
            status: unknown_code as i32,
            rows_affected: None,
            tx_id: None,
            request_id: Some(10),
            session_id: Some(20),
            trace_id: Some(format!("trace-unknown-status-{unknown_code}")),
            transaction_outcome: 1,
            durable_lsn: None,
            result_row_counts: vec![],
        };

        let decoded = RpcCompletion::decode(completion.encode_to_vec().as_slice())
            .expect("unknown enum value must decode as raw protobuf integer");

        assert_eq!(decoded.status, unknown_code as i32);
        assert!(
            RpcCompletionStatus::from_terminal_code(unknown_code).is_none(),
            "unknown protobuf completion status {unknown_code} must not map to a domain status"
        );
    }
}

#[test]
fn generated_rpc_completion_boundary_validation_rejects_unknown_codes_without_panic() {
    let valid = valid_generated_completion();
    validate_generated_rpc_completion(&valid)
        .expect("valid generated completion must satisfy boundary validation");

    for (field, invalid) in [
        (
            "status unspecified",
            RpcCompletion {
                status: 0,
                ..valid.clone()
            },
        ),
        (
            "status unknown",
            RpcCompletion {
                status: 12,
                ..valid.clone()
            },
        ),
        (
            "status negative",
            RpcCompletion {
                status: -1,
                ..valid.clone()
            },
        ),
        (
            "transaction outcome unspecified",
            RpcCompletion {
                transaction_outcome: 0,
                ..valid.clone()
            },
        ),
        (
            "transaction outcome unknown",
            RpcCompletion {
                transaction_outcome: 99,
                ..valid.clone()
            },
        ),
    ] {
        assert!(
            validate_generated_rpc_completion(&invalid).is_err(),
            "{field} must be rejected by generated completion boundary validation"
        );
    }

    let missing_lsn = RpcCompletion {
        durable_lsn: None,
        ..valid
    };
    assert!(
        validate_generated_rpc_completion(&missing_lsn).is_err(),
        "committed completion requires durable LSN evidence"
    );
}

#[test]
fn test_field_presence_preservation() {
    let completion_with_lsn = RpcCompletion {
        status: 1,
        rows_affected: None,
        tx_id: None,
        request_id: None,
        session_id: None,
        trace_id: None,
        transaction_outcome: 2,
        durable_lsn: Some(12345),
        result_row_counts: vec![],
    };

    let completion_without_lsn = RpcCompletion {
        status: 1,
        rows_affected: None,
        tx_id: None,
        request_id: None,
        session_id: None,
        trace_id: None,
        transaction_outcome: 2,
        durable_lsn: None,
        result_row_counts: vec![],
    };

    let bytes_with = completion_with_lsn.encode_to_vec();
    let bytes_without = completion_without_lsn.encode_to_vec();

    assert_ne!(
        bytes_with, bytes_without,
        "Optional field presence must affect encoding"
    );

    let decoded_with = RpcCompletion::decode(bytes_with.as_slice()).unwrap();
    let decoded_without = RpcCompletion::decode(bytes_without.as_slice()).unwrap();

    assert_eq!(decoded_with.durable_lsn, Some(12345));
    assert_eq!(decoded_without.durable_lsn, None);
}
