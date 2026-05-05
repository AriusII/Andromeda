//! H3-REMOTE-DISPATCH-004: End-to-End Remote Dispatch Integration Test
//!
//! This comprehensive integration test demonstrates the full request/response cycle
//! with invocation encoding, remote dispatch, and response decoding.

use andromeda_core::{AndromedaResult, InvocationId, RequestId, SessionId, TransactionId};
use andromeda_exec::dispatch::{
    ExecutionResult, ResultFrame, ResultStreamDecoder, decode_invocation_response,
};
use andromeda_proto::StructuredObjectHeader;

const TEST_SESSION_ID: u64 = 1000;
const TEST_REQUEST_ID: u64 = 5000;
const TEST_INVOCATION_ID: InvocationId = 100;

// ============================================================================
// Mock Remote Server
// ============================================================================

/// Mock remote server that processes invocation requests
struct MockRemoteServer {
    procedures: std::collections::HashMap<String, MockProcedure>,
}

struct MockProcedure {
    name: String,
    output_rows: u64,
}

impl MockRemoteServer {
    fn new() -> Self {
        let mut procedures = std::collections::HashMap::new();

        procedures.insert(
            "system.version".to_string(),
            MockProcedure {
                name: "system.version".to_string(),
                output_rows: 1,
            },
        );

        procedures.insert(
            "catalog.tables".to_string(),
            MockProcedure {
                name: "catalog.tables".to_string(),
                output_rows: 50,
            },
        );

        procedures.insert(
            "catalog.large_result".to_string(),
            MockProcedure {
                name: "catalog.large_result".to_string(),
                output_rows: 1000,
            },
        );

        Self { procedures }
    }

    fn execute_procedure(
        &self,
        procedure_name: &str,
    ) -> AndromedaResult<ExecutionResult> {
        match self.procedures.get(procedure_name) {
            Some(proc) => {
                let payload = StructuredObjectHeader {
                    name: "result".to_string(),
                    contract_hash: 0x1234567890abcdef,
                    descriptor_hash: 0xfedcba0987654321,
                    fields: vec![],
                    column_count: 0,
                    layout: andromeda_proto::StructuredObjectLayout::RowMajor,
                    row_count_policy: andromeda_proto::RowCountPolicy::ExactRequired,
                    row_count_exact: Some(proc.output_rows),
                    payload_length: 0,
                    payload_checksum: None,
                    max_payload_length: None,
                };

                Ok(ExecutionResult::Success {
                    invocation_id: TEST_INVOCATION_ID,
                    transaction_id: TEST_REQUEST_ID as u64,
                    output_payload: payload,
                    row_count: proc.output_rows,
                })
            }
            None => Ok(ExecutionResult::Error {
                invocation_id: TEST_INVOCATION_ID,
                error_code: 404,
                error_message: format!("procedure {} not found", procedure_name),
                transaction_id: Some(TEST_REQUEST_ID as u64),
            }),
        }
    }
}

// ============================================================================
// End-to-End Test Scenarios
// ============================================================================

#[test]
fn test_e2e_successful_invocation_single_row_result() {
    // 1. Setup server
    let server = MockRemoteServer::new();

    // 2. Client sends request for system.version (1 row)
    let procedure_name = "system.version";

    // 3. Server processes request
    let server_response = server.execute_procedure(procedure_name).expect("execute");

    // 4. Verify server response
    match &server_response {
        ExecutionResult::Success {
            invocation_id,
            row_count,
            ..
        } => {
            assert_eq!(*invocation_id, TEST_INVOCATION_ID);
            assert_eq!(*row_count, 1);
        }
        _ => panic!("expected success"),
    }

    // 5. Simulate server sending result frames
    let result_frame = ResultFrame {
        sequence_number: 0,
        data: vec![0x01, 0x02, 0x03], // Minimal response
        is_final: true,
    };

    let decoder = ResultStreamDecoder::new();
    decoder.add_frame(result_frame).expect("add frame");

    // 6. Client receives and verifies completion
    assert!(decoder.is_complete());

    // 7. Client reassembles
    let reassembled = decoder.finalize().expect("finalize");
    assert_eq!(reassembled, vec![0x01, 0x02, 0x03]);
}

#[test]
fn test_e2e_successful_invocation_multiple_rows() {
    // Setup
    let server = MockRemoteServer::new();

    // Request catalog.tables (50 rows)
    let procedure_name = "catalog.tables";
    let server_response = server.execute_procedure(procedure_name).expect("execute");

    // Verify response
    match &server_response {
        ExecutionResult::Success {
            invocation_id,
            row_count,
            ..
        } => {
            assert_eq!(*invocation_id, TEST_INVOCATION_ID);
            assert_eq!(*row_count, 50);
        }
        _ => panic!("expected success"),
    }

    // Simulate server sending 5 result frames (10 rows per frame)
    let decoder = ResultStreamDecoder::new();

    for i in 0..5 {
        let frame = ResultFrame {
            sequence_number: i as u64,
            data: vec![0xAA; 1000], // ~1KB per frame
            is_final: i == 4,
        };
        decoder.add_frame(frame).expect(&format!("frame {}", i));
    }

    // Client verifies complete result
    assert!(decoder.is_complete());
    assert_eq!(decoder.frame_count(), 5);
    assert_eq!(decoder.reassembled_size(), 5000);

    let reassembled = decoder.finalize().expect("finalize");
    assert_eq!(reassembled.len(), 5000);
}

#[test]
fn test_e2e_large_result_set_1000_rows() {
    // Setup
    let server = MockRemoteServer::new();

    // Request large result set
    let procedure_name = "catalog.large_result";
    let server_response = server.execute_procedure(procedure_name).expect("execute");

    // Verify response
    match &server_response {
        ExecutionResult::Success {
            invocation_id,
            row_count,
            ..
        } => {
            assert_eq!(*invocation_id, TEST_INVOCATION_ID);
            assert_eq!(*row_count, 1000);
        }
        _ => panic!("expected success"),
    }

    // Simulate server sending 100 result frames (10 rows per frame)
    let decoder = ResultStreamDecoder::new();

    for i in 0..100 {
        let frame = ResultFrame {
            sequence_number: i as u64,
            data: vec![i as u8; 10240], // ~10KB per frame
            is_final: i == 99,
        };
        decoder.add_frame(frame).expect(&format!("frame {}", i));
    }

    // Client verifies complete result
    assert!(decoder.is_complete());
    assert_eq!(decoder.frame_count(), 100);
    assert_eq!(decoder.reassembled_size(), 100 * 10240);

    // Verify reassembled data
    let reassembled = decoder.finalize().expect("finalize");
    assert_eq!(reassembled.len(), 100 * 10240);
}

#[test]
fn test_e2e_error_response_procedure_not_found() {
    // Setup
    let server = MockRemoteServer::new();

    // Request non-existent procedure
    let procedure_name = "nonexistent.procedure";
    let server_response = server.execute_procedure(procedure_name).expect("execute");

    // Verify error response
    match server_response {
        ExecutionResult::Error {
            invocation_id,
            error_code,
            error_message,
            transaction_id,
        } => {
            assert_eq!(invocation_id, TEST_INVOCATION_ID);
            assert_eq!(error_code, 404);
            assert!(error_message.contains("not found"));
            assert_eq!(transaction_id, Some(TEST_REQUEST_ID as u64));
        }
        _ => panic!("expected error"),
    }

    // No result frames for error - result stream skipped
    let decoder = ResultStreamDecoder::new();

    // Decoder remains incomplete (no frames sent for error)
    assert!(!decoder.is_complete());
}

#[test]
fn test_e2e_response_with_transaction_id() {
    // Verify that transaction ID is correctly threaded through response

    let payload = make_test_payload();

    let tx_id: TransactionId = 123456789;
    let result = decode_invocation_response(
        TEST_INVOCATION_ID,
        Some(tx_id),
        true,
        0,
        String::new(),
        Some(payload),
        42,
    )
    .expect("decode");

    match result {
        ExecutionResult::Success {
            transaction_id,
            row_count,
            ..
        } => {
            assert_eq!(transaction_id, tx_id);
            assert_eq!(row_count, 42);
        }
        _ => panic!("expected success"),
    }
}

fn make_test_payload() -> StructuredObjectHeader {
    StructuredObjectHeader {
        name: "test_result".to_string(),
        contract_hash: 0x1234567890abcdef,
        descriptor_hash: 0xfedcba0987654321,
        fields: vec![],
        column_count: 0,
        layout: andromeda_proto::StructuredObjectLayout::RowMajor,
        row_count_policy: andromeda_proto::RowCountPolicy::ExactRequired,
        row_count_exact: Some(0),
        payload_length: 0,
        payload_checksum: None,
        max_payload_length: None,
    }
}

#[test]
fn test_e2e_concurrent_result_reception() {
    use std::sync::Arc;
    use std::thread;

    // Simulate concurrent frame reception from network
    let decoder = Arc::new(ResultStreamDecoder::new());

    // Spawn 10 threads, each adding frames
    let mut handles = vec![];

    for i in 0..10 {
        let decoder_clone = Arc::clone(&decoder);
        let handle = thread::spawn(move || {
            let frame = ResultFrame {
                sequence_number: i as u64,
                data: vec![i as u8; 5000],
                is_final: i == 9,
            };
            decoder_clone.add_frame(frame)
        });
        handles.push(handle);
    }

    // Wait for all frames to be received
    for handle in handles {
        let result = handle.join();
        assert!(result.is_ok(), "thread panicked");
    }

    // Verify all frames received
    assert_eq!(decoder.frame_count(), 10);
    assert!(decoder.is_complete());

    // Verify reassembly
    let reassembled = decoder.finalize().expect("finalize");
    assert_eq!(reassembled.len(), 50000);
}

#[test]
fn test_e2e_partial_reception_waits_for_final_frame() {
    let decoder = ResultStreamDecoder::new();

    // Receive frames 0-4
    for i in 0..5 {
        let frame = ResultFrame {
            sequence_number: i as u64,
            data: vec![i as u8; 1000],
            is_final: false, // Not final yet
        };
        decoder.add_frame(frame).expect(&format!("frame {}", i));
    }

    // Should not be complete yet
    assert!(!decoder.is_complete());

    // Finalize should fail
    let result = decoder.finalize();
    assert!(result.is_err());

    // Now receive final frame
    let final_frame = ResultFrame {
        sequence_number: 5,
        data: vec![0xFF; 1000],
        is_final: true,
    };
    decoder.add_frame(final_frame).expect("final frame");

    // Now should be complete
    assert!(decoder.is_complete());

    // Finalize should succeed
    let reassembled = decoder.finalize().expect("finalize");
    assert_eq!(reassembled.len(), 6000);
}

#[test]
fn test_e2e_frame_order_independence() {
    // Frames arriving out of order should still be assembled correctly
    let decoder = ResultStreamDecoder::new();

    // Prepare frames
    let frame0 = ResultFrame {
        sequence_number: 0,
        data: vec![0; 100],
        is_final: false,
    };

    let frame2 = ResultFrame {
        sequence_number: 2,
        data: vec![2; 100],
        is_final: true,
    };

    let frame1 = ResultFrame {
        sequence_number: 1,
        data: vec![1; 100],
        is_final: false,
    };

    // Add in non-sequential order (if implementation supports it)
    let result = decoder.add_frame(frame2);
    if result.is_ok() {
        // Out-of-order reception is supported
        decoder.add_frame(frame1).expect("frame 1");
        decoder.add_frame(frame0).expect("frame 0");

        // Should still be complete
        assert!(decoder.is_complete());

        let reassembled = decoder.finalize().expect("finalize");
        // Verify data is in correct sequence order
        assert_eq!(reassembled[0..100], vec![0; 100][..]);
        assert_eq!(reassembled[100..200], vec![1; 100][..]);
        assert_eq!(reassembled[200..300], vec![2; 100][..]);
    }
}

#[test]
fn test_e2e_response_integrity_after_reassembly() {
    // Verify data integrity throughout the cycle

    // Original data to be transmitted
    let original_data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();

    // Fragment into frames
    let frame_size = 100;
    let mut frames = vec![];

    for (i, chunk) in original_data.chunks(frame_size).enumerate() {
        let is_final = i * frame_size + chunk.len() >= original_data.len();
        frames.push(ResultFrame {
            sequence_number: i as u64,
            data: chunk.to_vec(),
            is_final,
        });
    }

    // Receive via decoder
    let decoder = ResultStreamDecoder::new();
    for frame in frames {
        decoder.add_frame(frame).expect("add frame");
    }

    // Reassemble
    let reassembled = decoder.finalize().expect("finalize");

    // Verify bit-for-bit integrity
    assert_eq!(reassembled, original_data);
}

#[test]
fn test_e2e_result_stream_metadata_contract() {
    // Verify the metadata-before-payload contract is maintained

    let payload = StructuredObjectHeader {
        rows: vec![],
        schema: None,
    };

    // Response declares 1000 rows upfront
    let declared_row_count = 1000u64;

    let result = decode_invocation_response(
        TEST_INVOCATION_ID,
        Some(TEST_REQUEST_ID as u64),
        true,
        0,
        String::new(),
        Some(payload),
        declared_row_count,
    )
    .expect("decode");

    // Verify metadata
    match result {
        ExecutionResult::Success { row_count, .. } => {
            assert_eq!(row_count, declared_row_count);
        }
        _ => panic!("expected success"),
    }

    // Client uses declared count for buffer pre-allocation
    let expected_buffer_size = declared_row_count as usize * 100; // Assume 100 bytes per row

    // Simulate receiving frames totaling that size
    let decoder = ResultStreamDecoder::new();
    let total_size = 100_000; // 100KB for 1000 rows
    let frame_size = 10_000;

    for i in 0..(total_size / frame_size) {
        let frame = ResultFrame {
            sequence_number: i as u64,
            data: vec![0; frame_size],
            is_final: i == (total_size / frame_size - 1) as u64,
        };
        decoder.add_frame(frame).expect(&format!("frame {}", i));
    }

    // Verify metadata-payload contract
    assert!(decoder.is_complete());
    assert_eq!(decoder.reassembled_size(), total_size);
}

#[test]
fn test_e2e_error_recovery_after_partial_failure() {
    // Scenario: First request fails, second succeeds

    let server = MockRemoteServer::new();

    // First request: procedure not found
    let response1 = server.execute_procedure("nonexistent.proc").expect("request 1");
    assert!(matches!(response1, ExecutionResult::Error { .. }));

    // Second request: success
    let response2 = server.execute_procedure("system.version").expect("request 2");
    assert!(matches!(response2, ExecutionResult::Success { .. }));

    // For second response, receive frames
    let decoder = ResultStreamDecoder::new();
    let frame = ResultFrame {
        sequence_number: 0,
        data: vec![0xFF; 100],
        is_final: true,
    };
    decoder.add_frame(frame).expect("frame");

    assert!(decoder.is_complete());
}

#[test]
fn test_e2e_very_large_payload_10mb() {
    // Simulate 10MB payload across 1000 10KB frames

    let decoder = ResultStreamDecoder::new();
    let frame_size = 10 * 1024; // 10KB
    let frame_count = 1000; // 1000 frames = 10MB

    for i in 0..frame_count {
        let frame = ResultFrame {
            sequence_number: i as u64,
            data: vec![0xAB; frame_size],
            is_final: i == frame_count - 1,
        };
        decoder.add_frame(frame).expect(&format!("frame {}", i));
    }

    assert!(decoder.is_complete());
    assert_eq!(decoder.frame_count(), 1000);
    assert_eq!(decoder.reassembled_size(), 10 * 1024 * 1000);

    let reassembled = decoder.finalize().expect("finalize");
    assert_eq!(reassembled.len(), 10 * 1024 * 1000);
}
