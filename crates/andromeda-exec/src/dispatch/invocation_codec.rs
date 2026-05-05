//! H3-REMOTE-DISPATCH-004: InvocationResponse Deserialization and ResultStream Handling
//!
//! This module provides codec functions for decoding invocation responses and reassembling
//! fragmented result streams across multiple QUIC frames.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId, RequestId,
    SessionId, TransactionId,
};
use andromeda_proto::StructuredObjectHeader;

/// Represents the result of an invocation execution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    /// Successful execution with output payload
    Success {
        invocation_id: InvocationId,
        transaction_id: TransactionId,
        output_payload: StructuredObjectHeader,
        row_count: u64,
    },
    /// Error during execution
    Error {
        invocation_id: InvocationId,
        error_code: u32,
        error_message: String,
        transaction_id: Option<TransactionId>,
    },
}

/// Frame representing a fragment of a result stream
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultFrame {
    pub sequence_number: u64,
    pub data: Vec<u8>,
    pub is_final: bool,
}

/// Decodes an invocation response into an ExecutionResult
///
/// This function reconstructs the execution result from an invocation response,
/// handling both success and error cases.
///
/// # Arguments
/// * `invocation_id` - The invocation ID for correlation
/// * `transaction_id` - Optional transaction ID for audit
/// * `success` - True if execution succeeded, false for error
/// * `error_code` - Error code (if error)
/// * `error_message` - Error message (if error)
/// * `output_payload` - Output payload (if success)
/// * `row_count` - Row count in result set
///
/// # Returns
/// * `Ok(ExecutionResult)` - Successfully decoded result
/// * `Err(AndromedaError)` - Decoding error or validation failure
pub fn decode_invocation_response(
    invocation_id: InvocationId,
    transaction_id: Option<TransactionId>,
    success: bool,
    error_code: u32,
    error_message: String,
    output_payload: Option<StructuredObjectHeader>,
    row_count: u64,
) -> AndromedaResult<ExecutionResult> {
    if success {
        let payload = output_payload.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "success response missing output_payload",
            )
        })?;

        Ok(ExecutionResult::Success {
            invocation_id,
            transaction_id: transaction_id.unwrap_or(0),
            output_payload: payload,
            row_count,
        })
    } else {
        Ok(ExecutionResult::Error {
            invocation_id,
            error_code,
            error_message,
            transaction_id,
        })
    }
}

/// Decodes a set of result frames into a complete result set
///
/// # Arguments
/// * `frames` - Vector of ResultFrame objects (should be in sequence order)
///
/// # Returns
/// * `Ok(Vec<StructuredObjectHeader>)` - Reassembled result rows
/// * `Err(AndromedaError)` - If frames are incomplete or corrupted
pub fn decode_result_stream(
    frames: Vec<ResultFrame>,
) -> AndromedaResult<Vec<StructuredObjectHeader>> {
    if frames.is_empty() {
        return Ok(Vec::new());
    }

    // Validate frame sequence completeness
    validate_frame_sequence(&frames)?;

    // Reassemble frames into a single byte buffer
    let mut reassembled = Vec::new();
    for frame in frames {
        reassembled.extend_from_slice(&frame.data);
    }

    // In a real implementation, deserialize from the reassembled buffer
    // For now, we return an empty result set (placeholder)
    Ok(Vec::new())
}

/// Validates that frames form a complete sequence with no gaps
fn validate_frame_sequence(frames: &[ResultFrame]) -> AndromedaResult<()> {
    if frames.is_empty() {
        return Ok(());
    }

    for (i, frame) in frames.iter().enumerate() {
        if frame.sequence_number != i as u64 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "frame sequence gap: expected {}, got {}",
                    i, frame.sequence_number
                ),
            ));
        }
    }

    // Verify final frame is marked
    if !frames.last().map(|f| f.is_final).unwrap_or(false) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "final frame not marked",
        ));
    }

    Ok(())
}

/// Thread-safe result stream decoder
///
/// Handles fragmented results across multiple QUIC frames and reassembles them
/// into complete rows. Supports concurrent frame reception and decoding.
#[derive(Debug, Clone)]
pub struct ResultStreamDecoder {
    inner: Arc<Mutex<ResultStreamDecoderInner>>,
}

#[derive(Debug)]
struct ResultStreamDecoderInner {
    /// Frames received so far, indexed by sequence number
    frames: BTreeMap<u64, ResultFrame>,
    /// Whether we've received the final frame
    received_final: bool,
    /// Total rows expected (if known)
    expected_row_count: Option<u64>,
    /// Received row count so far
    received_row_count: u64,
}

impl ResultStreamDecoder {
    /// Creates a new result stream decoder
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ResultStreamDecoderInner {
                frames: BTreeMap::new(),
                received_final: false,
                expected_row_count: None,
                received_row_count: 0,
            })),
        }
    }

    /// Creates a new decoder with an expected row count
    pub fn with_expected_count(count: u64) -> Self {
        let mut decoder = Self::new();
        {
            let mut inner = decoder.inner.lock().unwrap();
            inner.expected_row_count = Some(count);
        }
        decoder
    }

    /// Adds a frame to the decoder
    ///
    /// # Arguments
    /// * `frame` - The frame to add
    ///
    /// # Returns
    /// * `Ok(())` - Frame added successfully
    /// * `Err(AndromedaError)` - Invalid frame or duplicate sequence number
    pub fn add_frame(&self, frame: ResultFrame) -> AndromedaResult<()> {
        let mut inner = self.inner.lock().unwrap();

        // Check for duplicate
        if inner.frames.contains_key(&frame.sequence_number) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!("duplicate frame sequence number: {}", frame.sequence_number),
            ));
        }

        // Check for gap
        if !inner.frames.is_empty() {
            let max_seq = inner.frames.keys().max().copied().unwrap_or(0);
            if frame.sequence_number > max_seq + 1 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    format!(
                        "frame sequence gap: max {} + 1, received {}",
                        max_seq, frame.sequence_number
                    ),
                ));
            }
        }

        if frame.is_final {
            inner.received_final = true;
        }

        inner.frames.insert(frame.sequence_number, frame);
        Ok(())
    }

    /// Checks if the stream is complete (all frames received)
    pub fn is_complete(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.received_final && inner.frames_are_contiguous()
    }

    /// Returns the reassembled frame data if complete
    pub fn finalize(&self) -> AndromedaResult<Vec<u8>> {
        let inner = self.inner.lock().unwrap();

        if !inner.received_final {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "stream not finalized: missing final frame",
            ));
        }

        if !inner.frames_are_contiguous() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "stream has gaps in sequence",
            ));
        }

        // Concatenate all frame data in order
        let mut result = Vec::new();
        for (_, frame) in &inner.frames {
            result.extend_from_slice(&frame.data);
        }

        Ok(result)
    }

    /// Returns the number of frames received
    pub fn frame_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.frames.len()
    }

    /// Returns the total reassembled size in bytes
    pub fn reassembled_size(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.frames.values().map(|f| f.data.len()).sum()
    }
}

impl ResultStreamDecoderInner {
    /// Checks if frame sequence numbers are contiguous from 0
    fn frames_are_contiguous(&self) -> bool {
        if self.frames.is_empty() {
            return true;
        }

        for i in 0..self.frames.len() {
            if !self.frames.contains_key(&(i as u64)) {
                return false;
            }
        }

        true
    }
}

impl Default for ResultStreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_decode_success_response() {
        let payload = make_test_payload();

        let result = decode_invocation_response(
            1,
            Some(42),
            true,
            0,
            String::new(),
            Some(payload.clone()),
            100,
        )
        .expect("decode");

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
    fn test_decode_error_response() {
        let result = decode_invocation_response(
            1,
            Some(42),
            false,
            500,
            "execution failed".to_string(),
            None,
            0,
        )
        .expect("decode");

        match result {
            ExecutionResult::Error {
                invocation_id,
                error_code,
                error_message,
                transaction_id,
            } => {
                assert_eq!(invocation_id, 1);
                assert_eq!(error_code, 500);
                assert_eq!(error_message, "execution failed");
                assert_eq!(transaction_id, Some(42));
            }
            _ => panic!("expected error result"),
        }
    }

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

        assert!(result.is_err()); // Should detect gap
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

        assert!(result.is_err()); // Should detect duplicate
    }

    #[test]
    fn test_result_stream_decoder_frame_count() {
        let decoder = ResultStreamDecoder::new();

        for i in 0..5 {
            let frame = ResultFrame {
                sequence_number: i as u64,
                data: vec![i as u8; 10],
                is_final: i == 4,
            };
            decoder.add_frame(frame).expect("add frame");
        }

        assert_eq!(decoder.frame_count(), 5);
        assert_eq!(decoder.reassembled_size(), 50);
    }
}
