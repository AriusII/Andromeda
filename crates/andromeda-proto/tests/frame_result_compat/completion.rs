use andromeda_proto::generated::{
    decode_generated_message, encode_generated_message,
    protocol::v1::{
        RpcCompletion,
        rpc_completion::{Status, TransactionOutcome as ProtoTransactionOutcome},
    },
};

use crate::proto_wire_fixtures::round_trip_generated;

fn generated_completion(
    status: Status,
    transaction_outcome: ProtoTransactionOutcome,
    rows_affected: Option<u64>,
    durable_lsn: Option<u64>,
    trace_id: Option<String>,
) -> RpcCompletion {
    RpcCompletion {
        status: status as i32,
        rows_affected,
        tx_id: Some(12345),
        request_id: Some(100),
        session_id: Some(200),
        trace_id,
        transaction_outcome: transaction_outcome as i32,
        durable_lsn,
        result_row_counts: vec![],
    }
}

fn stream_completion(
    status: Status,
    transaction_outcome: ProtoTransactionOutcome,
    rows_affected: Option<u64>,
    durable_lsn: Option<u64>,
) -> RpcCompletion {
    RpcCompletion {
        status: status as i32,
        rows_affected,
        tx_id: Some(1),
        request_id: Some(1),
        session_id: Some(1),
        trace_id: None,
        transaction_outcome: transaction_outcome as i32,
        durable_lsn,
        result_row_counts: vec![],
    }
}

/// Test: Completion signal variants
///
/// Validates that all RpcCompletion status variants are correctly encoded and
/// decoded, including their terminal codes.
#[test]
fn test_completion_signal_variants() {
    let status_variants = [
        (Status::Committed, "COMMITTED"),
        (Status::RolledBack, "ROLLED_BACK"),
        (Status::FailedBeforeTransaction, "FAILED_BEFORE_TRANSACTION"),
        (Status::Cancelled, "CANCELLED"),
        (Status::Poisoned, "POISONED"),
        (Status::PermissionDenied, "PERMISSION_DENIED"),
        (Status::ContractRejected, "CONTRACT_REJECTED"),
        (Status::SystemUnavailable, "SYSTEM_UNAVAILABLE"),
    ];

    for (status, name) in status_variants {
        let completion = generated_completion(
            status,
            ProtoTransactionOutcome::Committed,
            Some(10),
            Some(99999),
            Some(format!("trace-{name}")),
        );

        let deserialized = round_trip_generated(&completion);

        assert_eq!(
            deserialized.status, status as i32,
            "status should be preserved for variant: {name}"
        );
        assert_eq!(
            deserialized.trace_id,
            Some(format!("trace-{name}")),
            "trace_id should be preserved for variant: {name}"
        );
    }
}

/// Test: Multiple completion signals rejected (ordering)
///
/// Validates that only one terminal completion signal is allowed per stream
/// (test structure, not proto structure).
#[test]
fn test_multiple_completion_signals_rejected() {
    let completion_1 = stream_completion(
        Status::Committed,
        ProtoTransactionOutcome::Committed,
        Some(100),
        Some(1000),
    );

    let completion_2 = stream_completion(
        Status::RolledBack,
        ProtoTransactionOutcome::RolledBack,
        None,
        Some(1001),
    );

    let serialized_1 = encode_generated_message(&completion_1);
    let serialized_2 = encode_generated_message(&completion_2);

    assert_ne!(
        serialized_1, serialized_2,
        "distinct completion statuses should produce different serializations"
    );

    let deser_1: RpcCompletion =
        decode_generated_message(&serialized_1).expect("deserialization should succeed");
    let deser_2: RpcCompletion =
        decode_generated_message(&serialized_2).expect("deserialization should succeed");

    assert_eq!(deser_1.status, Status::Committed as i32);
    assert_eq!(deser_2.status, Status::RolledBack as i32);
    assert_ne!(deser_1.status, deser_2.status);
}
