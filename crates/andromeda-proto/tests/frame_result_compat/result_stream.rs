use andromeda_proto::generated::{
    contract::v1::{
        ColumnDescriptor, ResultStreamDescriptor, result_stream_descriptor::Cardinality,
        result_stream_descriptor::RowCountRequirement,
    },
    protocol::v1::{RpcBatch, RpcMetadata},
};

use crate::proto_wire_fixtures::round_trip_generated;

/// Test: Result stream metadata round-trip (RpcMetadata)
///
/// Validates that RpcMetadata describing result schema is preserved across
/// serialization boundaries.
#[test]
fn test_result_stream_metadata_round_trip() {
    let descriptor = ResultStreamDescriptor {
        stream_name: "results".to_string(),
        columns: vec![
            ColumnDescriptor {
                name: "id".to_string(),
                ordinal: 0,
                type_name: "INT64".to_string(),
            },
            ColumnDescriptor {
                name: "value".to_string(),
                ordinal: 1,
                type_name: "TEXT".to_string(),
            },
        ],
        cardinality: Cardinality::ZeroOrMore as i32,
        row_count_requirement: RowCountRequirement::ExactRequired as i32,
        row_count_exact: Some(42),
        row_count_max: None,
    };

    let metadata = RpcMetadata {
        result_streams: vec![descriptor],
        completion_policy: None,
    };

    let deserialized = round_trip_generated(&metadata);

    assert_eq!(
        deserialized.result_streams.len(),
        1,
        "result streams count should be preserved"
    );
    assert_eq!(
        deserialized.result_streams[0].stream_name, "results",
        "stream name should be preserved"
    );
    assert_eq!(
        deserialized.result_streams[0].columns.len(),
        2,
        "column count should be preserved"
    );
    assert_eq!(
        deserialized.result_streams[0].row_count_exact,
        Some(42),
        "row count exact should be preserved"
    );
    assert_eq!(
        deserialized.result_streams[0].columns[0].name, "id",
        "first column name should be preserved"
    );
    assert_eq!(
        deserialized.result_streams[0].columns[1].name, "value",
        "second column name should be preserved"
    );
}

/// Test: Row count exact encoding
///
/// Validates that row_count_exact in RpcBatch is correctly encoded and decoded,
/// including edge cases (0, very large numbers).
#[test]
fn test_row_count_exact_encoding() {
    let test_cases = [
        ("zero rows", 0u64),
        ("single row", 1u64),
        ("small batch", 100u64),
        ("large batch", 1_000_000u64),
        ("max u64", u64::MAX),
    ];

    for (name, row_count) in test_cases {
        let batch = RpcBatch {
            result_name: "result".to_string(),
            batch_index: 1,
            rows_emitted: row_count,
            structured_payload: vec![],
            row_count_exact: Some(row_count),
            terminal_batch: true,
        };

        let deserialized = round_trip_generated(&batch);

        assert_eq!(
            deserialized.row_count_exact,
            Some(row_count),
            "row count exact should be preserved for case: {name}"
        );
    }
}

/// Test: RpcBatch field validation
///
/// Validates that RpcBatch with structured payload is correctly encoded
/// including result name, batch index, and terminal flags.
#[test]
fn test_rpc_batch_field_validation() {
    let payload_data = b"binary_payload_data_here".to_vec();

    let batch = RpcBatch {
        result_name: "my_result".to_string(),
        batch_index: 5,
        rows_emitted: 50,
        structured_payload: payload_data.clone(),
        row_count_exact: Some(50),
        terminal_batch: false,
    };

    let deserialized = round_trip_generated(&batch);

    assert_eq!(deserialized.result_name, "my_result");
    assert_eq!(deserialized.batch_index, 5);
    assert_eq!(deserialized.rows_emitted, 50);
    assert_eq!(deserialized.structured_payload, payload_data);
    assert_eq!(deserialized.row_count_exact, Some(50));
    assert!(!deserialized.terminal_batch);
}
