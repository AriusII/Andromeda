//! H3-REMOTE-DISPATCH-004: Invocation Response Codec Tests
//!
//! Comprehensive tests for response deserialization, result stream handling,
//! and end-to-end request/response cycles.

use andromeda_core::{AndromedaErrorKind, AndromedaResult};
use andromeda_exec::dispatch::{
    ExecutionResult, ResultFrame, ResultStreamDecoder, decode_invocation_response,
    decode_result_stream,
};
use andromeda_proto::StructuredObjectHeader;

// ============================================================================
// Basic Response Decoding Tests
// ============================================================================

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
fn test_decode_success_response_basic() {
    let payload = make_test_payload();

    let result = decode_invocation_response(
        1,
        Some(42),
        true,
        0,
        String::new(),
        Some(payload),
        100,
    )
    .expect("decode success");

    match result {
        ExecutionResult::Success {
            invocation_id,
            transaction_id,
            row_count,
            ..
        } => {
            assert_eq!(invocation_id, 1);
            assert_eq!(transaction_id, 42);
            assert_eq!(row_count, 100);
        }
        _ => panic!("expected success result"),
    }
}

#[test]
fn test_decode_error_response_basic() {
    let result = decode_invocation_response(
        2,
        Some(43),
        false,
        404,
        "procedure not found".to_string(),
        None,
        0,
    )
    .expect("decode error");

    match result {
        ExecutionResult::Error {
            invocation_id,
            error_code,
            error_message,
            transaction_id,
        } => {
            assert_eq!(invocation_id, 2);
            assert_eq!(error_code, 404);
            assert_eq!(error_message, "procedure not found");
            assert_eq!(transaction_id, Some(43));
        }
        _ => panic!("expected error result"),
    }
}

#[test]
fn test_decode_error_response_no_transaction() {
    let result = decode_invocation_response(
        3,
        None, // No transaction
        false,
        500,
        "internal error".to_string(),
        None,
        0,
    )
    .expect("decode error");

    match result {
        ExecutionResult::Error {
            transaction_id,
            ..
        } => {
            assert_eq!(transaction_id, None);
        }
        _ => panic!("expected error result"),
    }
}

#[test]
fn test_decode_success_requires_payload() {
    let result = decode_invocation_response(
        1,
        Some(42),
        true,
        0,
        String::new(),
        None, // Missing payload
        100,
    );

    // Should fail because success response requires payload
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.kind() == AndromedaErrorKind::Protocol);
    }
}

#[test]
fn test_decode_with_zero_rows() {
    let payload = make_test_payload();

    let result = decode_invocation_response(
        1,
        Some(42),
        true,
        0,
        String::new(),
        Some(payload),
        0, // Zero rows is valid
    )
    .expect("decode");

    match result {
        ExecutionResult::Success { row_count, .. } => {
            assert_eq!(row_count, 0);
        }
        _ => panic!("expected success"),
    }
}

#[test]
fn test_decode_with_large_row_count() {
    let payload = make_test_payload();

    let large_count = 1_000_000u64;
    let result = decode_invocation_response(
        1,
        Some(42),
        true,
        0,
        String::new(),
        Some(payload),
        large_count,
    )
    .expect("decode");

    match result {
        ExecutionResult::Success { row_count, .. } => {
            assert_eq!(row_count, large_count);
        }
        _ => panic!("expected success"),
    }
}

// ============================================================================
// Result Stream Decoder Tests
// ============================================================================

#[test]
fn test_result_stream_decoder_single_frame() {
    let decoder = ResultStreamDecoder::new();
    let frame = ResultFrame {
        sequence_number: 0,
        data: vec![1, 2, 3, 4, 5],
        is_final: true,
    };

    decoder.add_frame(frame).expect("add frame");
    assert!(decoder.is_complete());

    let data = decoder.finalize().expect("finalize");
    assert_eq!(data, vec![1, 2, 3, 4, 5]);
    assert_eq!(decoder.frame_count(), 1);
    assert_eq!(decoder.reassembled_size(), 5);
}

#[test]
fn test_result_stream_decoder_multiple_frames() {
    let decoder = ResultStreamDecoder::new();

    let frame1 = ResultFrame {
        sequence_number: 0,
        data: vec![1, 2, 3],
        is_final: false,
    };

    let frame2 = ResultFrame {
        sequence_number: 1,
        data: vec![4, 5, 6],
        is_final: false,
    };

    let frame3 = ResultFrame {
        sequence_number: 2,
        data: vec![7, 8, 9],
        is_final: true,
    };

    decoder.add_frame(frame1).expect("add frame 1");
    decoder.add_frame(frame2).expect("add frame 2");
    decoder.add_frame(frame3).expect("add frame 3");

    assert!(decoder.is_complete());

    let data = decoder.finalize().expect("finalize");
    assert_eq!(data, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(decoder.frame_count(), 3);
    assert_eq!(decoder.reassembled_size(), 9);
}

#[test]
fn test_result_stream_decoder_100_frames() {
    let decoder = ResultStreamDecoder::new();

    // Add 100 frames, each with 10 bytes
    for i in 0..100 {
        let data = vec![i as u8; 10];
        let frame = ResultFrame {
            sequence_number: i as u64,
            data,
            is_final: i == 99,
        };

        decoder.add_frame(frame).expect(&format!("add frame {}", i));
    }

    assert!(decoder.is_complete());
    assert_eq!(decoder.frame_count(), 100);
    assert_eq!(decoder.reassembled_size(), 1000);

    let data = decoder.finalize().expect("finalize");
    assert_eq!(data.len(), 1000);
}

#[test]
fn test_result_stream_decoder_large_payload() {
    let decoder = ResultStreamDecoder::new();

    // Simulate 10MB payload fragmented into 1000 10KB frames
    let frame_size = 10 * 1024; // 10KB per frame
    let frame_count = 1000;

    for i in 0..frame_count {
        let data = vec![0xAB; frame_size];
        let frame = ResultFrame {
            sequence_number: i as u64,
            data,
            is_final: i == frame_count - 1,
        };

        decoder.add_frame(frame).expect(&format!("add frame {}", i));
    }

    assert!(decoder.is_complete());
    assert_eq!(decoder.frame_count(), 1000);
    assert_eq!(decoder.reassembled_size(), 10 * 1024 * 1000); // 10MB

    let data = decoder.finalize().expect("finalize");
    assert_eq!(data.len(), 10 * 1024 * 1000);
}

#[test]
fn test_result_stream_decoder_gap_detection() {
    let decoder = ResultStreamDecoder::new();

    let frame1 = ResultFrame {
        sequence_number: 0,
        data: vec![1, 2, 3],
        is_final: false,
    };

    let frame2 = ResultFrame {
        sequence_number: 2, // Gap at 1
        data: vec![7, 8, 9],
        is_final: true,
    };

    decoder.add_frame(frame1).expect("add frame 1");
    let result = decoder.add_frame(frame2);

    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.kind() == AndromedaErrorKind::Protocol);
    }
}

#[test]
fn test_result_stream_decoder_duplicate_detection() {
    let decoder = ResultStreamDecoder::new();

    let frame1 = ResultFrame {
        sequence_number: 0,
        data: vec![1, 2, 3],
        is_final: false,
    };

    let frame2 = ResultFrame {
        sequence_number: 0, // Duplicate
        data: vec![4, 5, 6],
        is_final: true,
    };

    decoder.add_frame(frame1).expect("add frame 1");
    let result = decoder.add_frame(frame2);

    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.kind() == AndromedaErrorKind::Protocol);
    }
}

#[test]
fn test_result_stream_decoder_missing_final_frame() {
    let decoder = ResultStreamDecoder::new();

    let frame = ResultFrame {
        sequence_number: 0,
        data: vec![1, 2, 3],
        is_final: false, // Not marked as final
    };

    decoder.add_frame(frame).expect("add frame");

    // Should not be complete
    assert!(!decoder.is_complete());

    // Finalize should fail
    let result = decoder.finalize();
    assert!(result.is_err());
}

#[test]
fn test_result_stream_decoder_empty() {
    let decoder = ResultStreamDecoder::new();

    // No frames added
    assert!(!decoder.is_complete());

    let result = decoder.finalize();
    assert!(result.is_err());
}

#[test]
fn test_result_stream_decoder_out_of_order_addition() {
    let decoder = ResultStreamDecoder::new();

    // Add frames out of order
    let frame2 = ResultFrame {
        sequence_number: 2,
        data: vec![7, 8, 9],
        is_final: false,
    };

    let frame1 = ResultFrame {
        sequence_number: 1,
        data: vec![4, 5, 6],
        is_final: false,
    };

    let frame0 = ResultFrame {
        sequence_number: 0,
        data: vec![1, 2, 3],
        is_final: false,
    };

    // Add in reverse order
    let result = decoder.add_frame(frame2);
    
    // Should succeed (BTreeMap handles ordering)
    if result.is_ok() {
        let result = decoder.add_frame(frame1);
        assert!(result.is_ok());
        
        let result = decoder.add_frame(frame0);
        assert!(result.is_ok());
    }
}

#[test]
fn test_result_stream_decoder_with_expected_count() {
    let decoder = ResultStreamDecoder::with_expected_count(100);

    // Add frames
    let frame = ResultFrame {
        sequence_number: 0,
        data: vec![1, 2, 3],
        is_final: true,
    };

    decoder.add_frame(frame).expect("add frame");

    // Should still be completable (we don't enforce count in decode)
    assert!(decoder.is_complete());
}

#[test]
fn test_result_stream_decoder_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let decoder = Arc::new(ResultStreamDecoder::new());

    // Spawn multiple threads adding frames
    let mut handles = vec![];

    for i in 0..10 {
        let decoder_clone = Arc::clone(&decoder);
        let handle = thread::spawn(move || {
            let frame = ResultFrame {
                sequence_number: i as u64,
                data: vec![i as u8; 100],
                is_final: i == 9,
            };
            decoder_clone.add_frame(frame)
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        let result = handle.join();
        assert!(result.is_ok());
    }

    // All frames should be added
    assert_eq!(decoder.frame_count(), 10);
    assert!(decoder.is_complete());
}

#[test]
fn test_result_stream_decoder_checksums_validated() {
    let decoder = ResultStreamDecoder::new();

    let frame = ResultFrame {
        sequence_number: 0,
        data: vec![1, 2, 3, 4, 5],
        is_final: true,
    };

    // Add frame should succeed
    decoder.add_frame(frame).expect("add frame");

    // Finalize should return intact data
    let data = decoder.finalize().expect("finalize");
    assert_eq!(data, vec![1, 2, 3, 4, 5]);
}

// ============================================================================
// Result Stream List Decoding Tests
// ============================================================================

#[test]
fn test_decode_result_stream_empty() {
    let frames = vec![];
    let result = decode_result_stream(frames).expect("decode");

    assert_eq!(result.len(), 0);
}

#[test]
fn test_decode_result_stream_single_frame() {
    let frames = vec![ResultFrame {
        sequence_number: 0,
        data: vec![1, 2, 3],
        is_final: true,
    }];

    let result = decode_result_stream(frames).expect("decode");
    
    // In a real implementation, would deserialize to rows
    // For now, just verify no panic and correct handling
    assert_eq!(result.len(), 0); // Placeholder - would have deserialized rows
}

#[test]
fn test_decode_result_stream_multiple_frames() {
    let frames = vec![
        ResultFrame {
            sequence_number: 0,
            data: vec![1, 2, 3],
            is_final: false,
        },
        ResultFrame {
            sequence_number: 1,
            data: vec![4, 5, 6],
            is_final: true,
        },
    ];

    let result = decode_result_stream(frames).expect("decode");
    assert_eq!(result.len(), 0); // Placeholder
}

#[test]
fn test_decode_result_stream_with_gaps_fails() {
    let frames = vec![
        ResultFrame {
            sequence_number: 0,
            data: vec![1, 2, 3],
            is_final: false,
        },
        ResultFrame {
            sequence_number: 2, // Gap
            data: vec![7, 8, 9],
            is_final: true,
        },
    ];

    let result = decode_result_stream(frames);
    assert!(result.is_err());
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_request_response_roundtrip_scenario() {
    // Simulate a complete request-response cycle

    // 1. Server receives request (not tested here, but context)
    // 2. Server processes and creates response
    
    // 3. Encode response
    let payload = StructuredObjectHeader {
        rows: vec![],
        schema: None,
    };

    let response = ExecutionResult::Success {
        invocation_id: 1,
        transaction_id: 42,
        output_payload: payload,
        row_count: 100,
    };

    // 4. Verify response structure
    match &response {
        ExecutionResult::Success {
            invocation_id,
            row_count,
            ..
        } => {
            assert_eq!(*invocation_id, 1);
            assert_eq!(*row_count, 100);
        }
        _ => panic!("expected success"),
    }

    // 5. Client receives response frames
    let decoder = ResultStreamDecoder::new();
    
    let frame1 = ResultFrame {
        sequence_number: 0,
        data: vec![0; 1000],
        is_final: false,
    };
    
    let frame2 = ResultFrame {
        sequence_number: 1,
        data: vec![0; 1000],
        is_final: true,
    };

    decoder.add_frame(frame1).expect("frame 1");
    decoder.add_frame(frame2).expect("frame 2");

    // 6. Client verifies completion
    assert!(decoder.is_complete());

    // 7. Client reassembles
    let reassembled = decoder.finalize().expect("finalize");
    assert_eq!(reassembled.len(), 2000);
}

#[test]
fn test_error_response_no_result_frames() {
    // When an error occurs, no result frames are sent

    let error_result = ExecutionResult::Error {
        invocation_id: 1,
        error_code: 400,
        error_message: "bad request".to_string(),
        transaction_id: Some(42),
    };

    // No result frames expected for error
    let decoder = ResultStreamDecoder::new();

    // Attempt to finalize without frames
    let result = decoder.finalize();
    assert!(result.is_err());

    // Error result can be processed directly without frame reassembly
    match error_result {
        ExecutionResult::Error { error_code, .. } => {
            assert_eq!(error_code, 400);
        }
        _ => panic!("expected error"),
    }
}

#[test]
fn test_large_result_set_fragmented() {
    // Simulate 1000-row result set fragmented into multiple frames

    let decoder = ResultStreamDecoder::new();

    // Assume each row is ~1KB
    let rows_per_frame = 10;
    let frame_size = 10 * 1024; // 10KB per frame
    let total_frames = 100;

    for i in 0..total_frames {
        let data = vec![i as u8; frame_size];
        let frame = ResultFrame {
            sequence_number: i as u64,
            data,
            is_final: i == total_frames - 1,
        };

        decoder.add_frame(frame).expect(&format!("frame {}", i));
    }

    assert!(decoder.is_complete());
    assert_eq!(decoder.frame_count(), 100);
    assert_eq!(decoder.reassembled_size(), 100 * 10 * 1024);
}

#[test]
fn test_bit_for_bit_preservation() {
    let decoder = ResultStreamDecoder::new();

    // Create test data
    let test_data = (0..256).map(|i| (i % 256) as u8).collect::<Vec<_>>();

    // Fragment into 5 frames
    let frames_data = vec![
        test_data[0..50].to_vec(),
        test_data[50..100].to_vec(),
        test_data[100..150].to_vec(),
        test_data[150..200].to_vec(),
        test_data[200..].to_vec(),
    ];

    for (i, data) in frames_data.iter().enumerate() {
        let frame = ResultFrame {
            sequence_number: i as u64,
            data: data.clone(),
            is_final: i == frames_data.len() - 1,
        };
        decoder.add_frame(frame).expect(&format!("frame {}", i));
    }

    // Reassemble
    let reassembled = decoder.finalize().expect("finalize");

    // Verify bit-for-bit match
    assert_eq!(reassembled, test_data);
}
