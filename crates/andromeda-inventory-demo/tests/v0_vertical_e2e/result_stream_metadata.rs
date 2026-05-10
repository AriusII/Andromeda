use crate::support::{
    context, encoded_execute_frame, executable_procedure, inventory_catalog_snapshot, request,
    stock,
};
use andromeda_error::AndromedaErrorKind;
use andromeda_inventory_demo::inventory_reserve_stock_contract;
use andromeda_inventory_demo::{V0InventoryRecoverableOutcome, V0InventoryRecoverableRuntime};
use andromeda_result_stream::CompletionStatus;
use andromeda_rpc_protocol::{
    FrameCodec, FrameType, StreamRole, validate_result_stream_sequence,
    validate_single_frame_on_stream,
};
use andromeda_wal::{InMemoryWal, Lsn};

const V0_METADATA_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.result-metadata.v1";
const V0_BATCH_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.inventory-reservation-batch.v1";
const V0_COMPLETION_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.completion.v1";

#[derive(Debug, PartialEq, Eq)]
struct DecodedV0Metadata {
    stream_id: u64,
    row_count_exact: u64,
    column_count: u32,
    exact_one_cardinality_marker: u8,
}

#[derive(Debug, PartialEq, Eq)]
struct DecodedV0Batch {
    product_id: i64,
    quantity: i64,
    remaining_quantity: i64,
    reserved: bool,
    rows_affected: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct DecodedV0Completion {
    rows_affected: u64,
    durable_lsn: Lsn,
}

fn execute_v0_inventory_reserve_stock(
    invocation_id: u64,
    trace_id: u64,
) -> V0InventoryRecoverableOutcome {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());

    runtime
        .execute_encoded_inventory_reserve_stock(
            &encoded_execute_frame(),
            &procedure,
            request(&contract, invocation_id),
            &context(&contract, trace_id),
            stock(),
        )
        .unwrap()
}

#[test]
fn v0_inventory_result_stream_metadata_precedes_batch_and_typed_completion() {
    let outcome = execute_v0_inventory_reserve_stock(718, 7018);

    assert_eq!(
        outcome.vertical.completion.status(),
        CompletionStatus::Committed
    );
    assert_eq!(outcome.vertical.completion.rows_affected(), Some(2));
    assert_eq!(outcome.vertical.completion.durable_lsn(), Some(Lsn::new(3)));
    assert_eq!(
        outcome
            .result_frames
            .iter()
            .map(|frame| frame.header.frame_type)
            .collect::<Vec<_>>(),
        vec![
            FrameType::RpcMetadata,
            FrameType::RpcBatch,
            FrameType::RpcCompletion,
        ]
    );
    assert!(
        outcome
            .result_frames
            .iter()
            .all(|frame| frame.header.payload_length > 0)
    );

    // V0 still carries compatibility raw ResultStream payloads. This packet proves the
    // binary frame boundary before applying compatibility sequence validation.
    let mut encoded_result_stream = Vec::new();
    for frame in &outcome.result_frames {
        encoded_result_stream.extend(FrameCodec::encode(frame).unwrap());
    }
    let scanned_result_frames = FrameCodec::scan_all(&encoded_result_stream).unwrap();
    assert_eq!(
        scanned_result_frames
            .iter()
            .map(|frame| frame.header.frame_type)
            .collect::<Vec<_>>(),
        vec![
            FrameType::RpcMetadata,
            FrameType::RpcBatch,
            FrameType::RpcCompletion,
        ]
    );

    validate_result_stream_sequence(&scanned_result_frames).unwrap();
    for frame in &scanned_result_frames {
        validate_single_frame_on_stream(frame, StreamRole::ResultUnidirectional).unwrap();
    }

    let metadata = decode_v0_metadata_payload(&scanned_result_frames[0].payload);
    assert_eq!(
        metadata,
        DecodedV0Metadata {
            stream_id: 1,
            row_count_exact: 1,
            column_count: 1,
            exact_one_cardinality_marker: 1,
        }
    );

    let batch = decode_v0_batch_payload(&scanned_result_frames[1].payload);
    assert_eq!(
        batch,
        DecodedV0Batch {
            product_id: 42,
            quantity: 3,
            remaining_quantity: 7,
            reserved: true,
            rows_affected: 2,
        }
    );

    let completion = decode_v0_completion_payload(&scanned_result_frames[2].payload);
    assert_eq!(
        completion,
        DecodedV0Completion {
            rows_affected: 2,
            durable_lsn: Lsn::new(3),
        }
    );
    assert!(
        !completion.durable_lsn.is_zero(),
        "ResultStream completion payload must carry non-zero durable LSN evidence"
    );
}

#[test]
fn v0_inventory_result_stream_rejects_batch_after_terminal_completion() {
    let outcome = execute_v0_inventory_reserve_stock(719, 7019);
    let mut frames = outcome.result_frames.clone();
    frames.push(outcome.result_frames[1].clone());

    let error = validate_result_stream_sequence(&frames)
        .expect_err("ResultStream sequence must reject payload after completion");

    assert_eq!(error.kind(), AndromedaErrorKind::Protocol);
    assert!(
        error.message().contains("batch must not follow completion"),
        "terminal frame ordering rejection should identify the late batch"
    );
}

fn decode_v0_metadata_payload(payload: &[u8]) -> DecodedV0Metadata {
    let fields = strip_v0_domain(payload, V0_METADATA_PAYLOAD_DOMAIN);
    assert_eq!(
        fields.len(),
        8 + 8 + 4 + 1,
        "V0 metadata payload field length changed"
    );

    DecodedV0Metadata {
        stream_id: read_u64_le(fields, 0),
        row_count_exact: read_u64_le(fields, 8),
        column_count: read_u32_le(fields, 16),
        exact_one_cardinality_marker: fields[20],
    }
}

fn decode_v0_batch_payload(payload: &[u8]) -> DecodedV0Batch {
    let fields = strip_v0_domain(payload, V0_BATCH_PAYLOAD_DOMAIN);
    assert_eq!(
        fields.len(),
        8 + 8 + 8 + 1 + 8,
        "V0 batch payload field length changed"
    );

    DecodedV0Batch {
        product_id: read_i64_le(fields, 0),
        quantity: read_i64_le(fields, 8),
        remaining_quantity: read_i64_le(fields, 16),
        reserved: match fields[24] {
            0 => false,
            1 => true,
            reserved => panic!("invalid V0 batch reserved marker {reserved}"),
        },
        rows_affected: read_u64_le(fields, 25),
    }
}

fn decode_v0_completion_payload(payload: &[u8]) -> DecodedV0Completion {
    let fields = strip_v0_domain(payload, V0_COMPLETION_PAYLOAD_DOMAIN);
    assert_eq!(
        fields.len(),
        8 + 8,
        "V0 completion payload field length changed"
    );

    DecodedV0Completion {
        rows_affected: read_u64_le(fields, 0),
        durable_lsn: Lsn::new(read_u64_le(fields, 8)),
    }
}

fn strip_v0_domain<'a>(payload: &'a [u8], domain: &[u8]) -> &'a [u8] {
    assert!(
        payload.starts_with(domain),
        "V0 payload domain tag changed or is corrupt"
    );
    assert_eq!(
        payload.get(domain.len()),
        Some(&0),
        "V0 payload domain separator changed"
    );
    &payload[domain.len() + 1..]
}

fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(read_fixed(bytes, offset))
}

fn read_i64_le(bytes: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(read_fixed(bytes, offset))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(read_fixed(bytes, offset))
}

fn read_fixed<const N: usize>(bytes: &[u8], offset: usize) -> [u8; N] {
    let end = offset + N;
    let mut out = [0; N];
    out.copy_from_slice(&bytes[offset..end]);
    out
}
