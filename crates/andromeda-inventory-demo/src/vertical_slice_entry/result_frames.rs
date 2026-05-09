use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId, TransactionId,
};
use andromeda_exec::VerticalInvocationOutcome;
use andromeda_rpc_protocol::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader, FrameType, ResultStreamMetadataPolicy,
    validate_result_stream_sequence_with_metadata_policy,
};
use andromeda_wal::Lsn;

use crate::ReserveStockEffect;

const V0_METADATA_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.result-metadata.v1";
const V0_BATCH_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.inventory-reservation-batch.v1";
const V0_COMPLETION_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.completion.v1";

pub(crate) fn encode_v0_result_frames(
    request_id: RequestId,
    session_id: SessionId,
    tx_id: Option<TransactionId>,
    effect: &ReserveStockEffect,
    vertical: &VerticalInvocationOutcome,
) -> AndromedaResult<Vec<FrameBytes>> {
    let completion = vertical.completion;
    let durable_lsn = completion.durable_lsn.ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "V0 committed completion requires a durable LSN",
        )
    })?;
    let rows_affected = completion.rows_affected.ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Contract,
            "V0 committed completion requires exact rows-affected evidence",
        )
    })?;
    let mut frames = vec![
        result_frame(
            FrameType::RpcMetadata,
            request_id,
            session_id,
            tx_id,
            encode_v0_metadata_payload(vertical)?,
        ),
        result_frame(
            FrameType::RpcBatch,
            request_id,
            session_id,
            tx_id,
            encode_v0_batch_payload(effect),
        ),
        result_frame(
            FrameType::RpcCompletion,
            request_id,
            session_id,
            tx_id,
            encode_v0_completion_payload(rows_affected, durable_lsn),
        ),
    ];

    for frame in &mut frames {
        frame.header.payload_length = frame.payload.len() as u64;
    }
    validate_result_stream_sequence_with_metadata_policy(
        &frames,
        ResultStreamMetadataPolicy::RowBatchRequired,
    )?;
    Ok(frames)
}

fn result_frame(
    frame_type: FrameType,
    request_id: RequestId,
    session_id: SessionId,
    tx_id: Option<TransactionId>,
    payload: Vec<u8>,
) -> FrameBytes {
    FrameBytes {
        header: FrameHeader {
            frame_type,
            request_id,
            session_id,
            tx_id,
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    }
}

fn encode_v0_metadata_payload(vertical: &VerticalInvocationOutcome) -> AndromedaResult<Vec<u8>> {
    let metadata = vertical.result_metadata;
    let row_count_exact = metadata.row_count_exact.ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Contract,
            "V0 metadata frame requires exact row-count evidence",
        )
    })?;
    let mut bytes = Vec::with_capacity(V0_METADATA_PAYLOAD_DOMAIN.len() + 1 + 32);
    bytes.extend_from_slice(V0_METADATA_PAYLOAD_DOMAIN);
    bytes.push(0);
    bytes.extend_from_slice(&metadata.stream_id.to_le_bytes());
    bytes.extend_from_slice(&row_count_exact.to_le_bytes());
    bytes.extend_from_slice(&metadata.column_count.to_le_bytes());
    bytes.push(1);
    Ok(bytes)
}

fn encode_v0_batch_payload(effect: &ReserveStockEffect) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(V0_BATCH_PAYLOAD_DOMAIN.len() + 1 + 33);
    bytes.extend_from_slice(V0_BATCH_PAYLOAD_DOMAIN);
    bytes.push(0);
    bytes.extend_from_slice(&effect.result.product_id.to_le_bytes());
    bytes.extend_from_slice(&effect.result.quantity.to_le_bytes());
    bytes.extend_from_slice(&effect.result.remaining_quantity.to_le_bytes());
    bytes.push(u8::from(effect.result.reserved));
    bytes.extend_from_slice(&effect.rows_affected.to_le_bytes());
    bytes
}

fn encode_v0_completion_payload(rows_affected: u64, durable_lsn: Lsn) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(V0_COMPLETION_PAYLOAD_DOMAIN.len() + 1 + 16);
    bytes.extend_from_slice(V0_COMPLETION_PAYLOAD_DOMAIN);
    bytes.push(0);
    bytes.extend_from_slice(&rows_affected.to_le_bytes());
    bytes.extend_from_slice(&durable_lsn.get().to_le_bytes());
    bytes
}
