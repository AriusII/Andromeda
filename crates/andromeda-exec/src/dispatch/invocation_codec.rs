//! Invocation response decoding and result stream frame reassembly.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId, TransactionId,
};
use andromeda_proto::StructuredObjectHeader;
#[cfg(test)]
use andromeda_proto::{RowCountPolicy, StructuredObjectLayout};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    Success {
        invocation_id: InvocationId,
        transaction_id: TransactionId,
        output_payload: StructuredObjectHeader,
        row_count: u64,
    },
    Error {
        invocation_id: InvocationId,
        error_code: u32,
        error_message: String,
        transaction_id: Option<TransactionId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultFrame {
    pub sequence_number: u64,
    pub data: Vec<u8>,
    pub is_final: bool,
}

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
            transaction_id: transaction_id.unwrap_or(TransactionId::new(0)),
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

pub fn decode_result_stream(
    frames: Vec<ResultFrame>,
) -> AndromedaResult<Vec<StructuredObjectHeader>> {
    if frames.is_empty() {
        return Ok(Vec::new());
    }

    validate_frame_sequence(&frames)?;

    let mut reassembled = Vec::new();
    for frame in frames {
        reassembled.extend_from_slice(&frame.data);
    }

    // Row decoding is supplied by the typed result codec once it is wired in.
    Ok(Vec::new())
}

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

    if !frames.last().map(|f| f.is_final).unwrap_or(false) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "final frame not marked",
        ));
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct ResultStreamDecoder {
    inner: Arc<Mutex<ResultStreamDecoderInner>>,
}

#[derive(Debug)]
struct ResultStreamDecoderInner {
    frames: BTreeMap<u64, ResultFrame>,
    received_final: bool,
    expected_row_count: Option<u64>,
}

impl ResultStreamDecoder {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ResultStreamDecoderInner {
                frames: BTreeMap::new(),
                received_final: false,
                expected_row_count: None,
            })),
        }
    }

    pub fn with_expected_count(count: u64) -> Self {
        let decoder = Self::new();
        {
            let mut inner = decoder.lock_inner();
            inner.expected_row_count = Some(count);
        }
        decoder
    }

    pub fn add_frame(&self, frame: ResultFrame) -> AndromedaResult<()> {
        let mut inner = self.lock_inner();

        if inner.frames.contains_key(&frame.sequence_number) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!("duplicate frame sequence number: {}", frame.sequence_number),
            ));
        }

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

    pub fn is_complete(&self) -> bool {
        let inner = self.lock_inner();
        inner.received_final && inner.frames_are_contiguous()
    }

    pub fn finalize(&self) -> AndromedaResult<Vec<u8>> {
        let inner = self.lock_inner();

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

        let mut result = Vec::new();
        for frame in inner.frames.values() {
            result.extend_from_slice(&frame.data);
        }

        Ok(result)
    }

    pub fn frame_count(&self) -> usize {
        let inner = self.lock_inner();
        inner.frames.len()
    }

    pub fn reassembled_size(&self) -> usize {
        let inner = self.lock_inner();
        inner.frames.values().map(|f| f.data.len()).sum()
    }

    fn lock_inner(&self) -> MutexGuard<'_, ResultStreamDecoderInner> {
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl ResultStreamDecoderInner {
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
            contract_hash: andromeda_core::ContractHash::test_vector(1),
            descriptor_hash: andromeda_core::ContractHash::test_vector(2),
            fields: vec![],
            column_count: 0,
            layout: StructuredObjectLayout::RowMajor,
            row_count_policy: RowCountPolicy::ExactRequired,
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
            InvocationId::new(1),
            Some(TransactionId::new(42)),
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
                assert_eq!(invocation_id, InvocationId::new(1));
                assert_eq!(transaction_id, TransactionId::new(42));
                assert_eq!(row_count, 100);
            }
            _ => panic!("expected success result"),
        }
    }

    #[test]
    fn test_decode_error_response() {
        let result = decode_invocation_response(
            InvocationId::new(1),
            Some(TransactionId::new(42)),
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
                assert_eq!(invocation_id, InvocationId::new(1));
                assert_eq!(error_code, 500);
                assert_eq!(error_message, "execution failed");
                assert_eq!(transaction_id, Some(TransactionId::new(42)));
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
