use andromeda_catalog::{CatalogSnapshot, ProcedureContract};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId, TransactionId,
};
use andromeda_observe::{
    CompletionEmittedTrace, EventCorrelation, EventEnvelope, EventId, EventSink,
    ProtocolCorrelation, TraceEvent,
};
use andromeda_quic::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameCodec, FrameHeader, FrameType,
    ResultStreamMetadataPolicy, StreamRole, validate_result_stream_sequence_with_metadata_policy,
};
use andromeda_srpl::{
    ExecutableProcedurePlan, bind_executable_procedure_plan, compile_narrow_procedure_signature,
};
use andromeda_storage::Lsn;

use crate::{
    InventoryReserveStockExecutor, InventoryStock, InvocationContext, InvocationRequest,
    InvocationWal, LocalVerticalRuntime, ReserveStockCommand, ReserveStockEffect,
    VerticalInvocationOutcome, transaction_id_for_invocation,
};

const V0_RPC_EXECUTE_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.inventory-reserve-stock-rpc.v1";
const V0_METADATA_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.result-metadata.v1";
const V0_BATCH_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.inventory-reservation-batch.v1";
const V0_COMPLETION_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.completion.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V0InventoryReserveStockRpcPayload {
    pub product_id: i64,
    pub quantity: i64,
}

impl V0InventoryReserveStockRpcPayload {
    pub fn new(product_id: i64, quantity: i64) -> AndromedaResult<Self> {
        let payload = Self {
            product_id,
            quantity,
        };
        payload.validate()?;
        Ok(payload)
    }

    pub fn from_command(command: ReserveStockCommand) -> AndromedaResult<Self> {
        command.validate()?;
        Self::new(command.product_id, command.quantity)
    }

    pub fn to_command(self) -> ReserveStockCommand {
        ReserveStockCommand {
            product_id: self.product_id,
            quantity: self.quantity,
        }
    }

    pub fn encode(self) -> AndromedaResult<Vec<u8>> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(V0_RPC_EXECUTE_PAYLOAD_DOMAIN.len() + 1 + 16);
        bytes.extend_from_slice(V0_RPC_EXECUTE_PAYLOAD_DOMAIN);
        bytes.push(0);
        bytes.extend_from_slice(&self.product_id.to_le_bytes());
        bytes.extend_from_slice(&self.quantity.to_le_bytes());
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> AndromedaResult<Self> {
        let expected_len = V0_RPC_EXECUTE_PAYLOAD_DOMAIN.len() + 1 + 16;
        if bytes.len() != expected_len {
            return Err(v0_protocol_error(
                "V0 Inventory.ReserveStock RPC payload length mismatch",
            ));
        }
        if !bytes.starts_with(V0_RPC_EXECUTE_PAYLOAD_DOMAIN)
            || bytes[V0_RPC_EXECUTE_PAYLOAD_DOMAIN.len()] != 0
        {
            return Err(v0_protocol_error(
                "V0 Inventory.ReserveStock RPC payload domain mismatch",
            ));
        }

        let data = &bytes[V0_RPC_EXECUTE_PAYLOAD_DOMAIN.len() + 1..];
        let product_id = i64::from_le_bytes(data[..8].try_into().expect("fixed product id"));
        let quantity = i64::from_le_bytes(data[8..16].try_into().expect("fixed quantity"));
        Self::new(product_id, quantity)
    }

    fn validate(self) -> AndromedaResult<()> {
        self.to_command().validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V0InventoryRecoverableOutcome {
    pub srpl_plan: ExecutableProcedurePlan,
    pub effect: ReserveStockEffect,
    pub vertical: VerticalInvocationOutcome,
    pub command_frame: FrameBytes,
    pub result_frames: Vec<FrameBytes>,
}

impl V0InventoryRecoverableOutcome {
    pub fn durable_lsn(&self) -> Option<Lsn> {
        self.vertical.completion.durable_lsn
    }
}

pub struct V0InventoryRecoverableRuntime<W> {
    local: LocalVerticalRuntime<W>,
}

impl<W> V0InventoryRecoverableRuntime<W>
where
    W: InvocationWal,
{
    pub const fn new(wal: W) -> Self {
        Self {
            local: LocalVerticalRuntime::new(wal),
        }
    }

    pub fn wal(&self) -> &W {
        self.local.wal()
    }

    pub fn wal_mut(&mut self) -> &mut W {
        self.local.wal_mut()
    }

    pub fn into_inner(self) -> LocalVerticalRuntime<W> {
        self.local
    }

    pub fn execute_encoded_inventory_reserve_stock(
        &mut self,
        encoded_execute_frame: &[u8],
        srpl_source: &str,
        catalog: &CatalogSnapshot,
        contract: &ProcedureContract,
        request: InvocationRequest,
        context: &InvocationContext,
        observed_stock: InventoryStock,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome> {
        let command_frame = decode_v0_execute_frame(encoded_execute_frame)?;
        let command =
            V0InventoryReserveStockRpcPayload::decode(&command_frame.payload)?.to_command();
        let srpl_ir = compile_narrow_procedure_signature(srpl_source).map_err(|diagnostic| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!(
                    "V0 Inventory.ReserveStock SRPL compilation failed during {:?}: {}",
                    diagnostic.phase, diagnostic.message
                ),
            )
        })?;
        let srpl_plan = bind_executable_procedure_plan(&srpl_ir, catalog)?;

        if srpl_plan.evidence.procedure_contract != contract.as_ref() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "V0 Inventory.ReserveStock SRPL plan contract evidence does not match the runtime contract",
            ));
        }

        let effect = InventoryReserveStockExecutor::reserve(command, observed_stock)?;
        let local_procedure = effect.to_local_procedure(contract)?;
        let vertical = self
            .local
            .execute_authorized(request, &local_procedure, context)?;
        let tx_id = vertical
            .completion
            .transaction_state
            .map(|_| transaction_id_for_invocation(vertical.completion.invocation_id));
        let result_frames = encode_v0_result_frames(
            command_frame.header.request_id,
            command_frame.header.session_id,
            tx_id,
            &effect,
            &vertical,
        )?;

        Ok(V0InventoryRecoverableOutcome {
            srpl_plan,
            effect,
            vertical,
            command_frame,
            result_frames,
        })
    }

    pub fn execute_encoded_inventory_reserve_stock_observed(
        &mut self,
        encoded_execute_frame: &[u8],
        srpl_source: &str,
        catalog: &CatalogSnapshot,
        contract: &ProcedureContract,
        request: InvocationRequest,
        context: &InvocationContext,
        observed_stock: InventoryStock,
        sink: &mut impl EventSink,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome> {
        let outcome = self.execute_encoded_inventory_reserve_stock(
            encoded_execute_frame,
            srpl_source,
            catalog,
            contract,
            request,
            context,
            observed_stock,
        )?;

        emit_v0_outcome_events(&outcome, sink)?;
        Ok(outcome)
    }
}

pub fn inventory_reserve_stock_v0_pdf_srpl_source() -> &'static str {
    "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId and Stock.AvailableQuantity >= Quantity else fail InsufficientStock; update Inventory.ProductStock set AvailableQuantity = Stock.AvailableQuantity - Quantity where ProductId = Stock.ProductId affected rows 1; return Reservation (Reserved); end;"
}

pub fn encode_inventory_reserve_stock_v0_execute_frame(
    request_id: RequestId,
    session_id: SessionId,
    payload: V0InventoryReserveStockRpcPayload,
) -> AndromedaResult<Vec<u8>> {
    let payload = payload.encode()?;
    FrameCodec::encode(&FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcExecuteRequest,
            request_id,
            session_id,
            tx_id: None,
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    })
}

pub fn decode_v0_execute_frame(encoded_execute_frame: &[u8]) -> AndromedaResult<FrameBytes> {
    let frame = FrameCodec::decode(encoded_execute_frame)?;
    frame.validate(StreamRole::CommandBidirectional)?;
    if frame.header.frame_type != FrameType::RpcExecuteRequest {
        return Err(v0_protocol_error(
            "V0 Inventory.ReserveStock requires an RPC execute request frame",
        ));
    }
    if frame.header.tx_id.is_some() {
        return Err(v0_protocol_error(
            "V0 RPC execute request must be pre-transaction and omit transaction evidence",
        ));
    }
    Ok(frame)
}

fn encode_v0_result_frames(
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
    let mut frames = vec![
        result_frame(
            FrameType::RpcMetadata,
            request_id,
            session_id,
            tx_id,
            encode_v0_metadata_payload(vertical),
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
            encode_v0_completion_payload(completion.rows_affected.unwrap_or_default(), durable_lsn),
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

fn encode_v0_metadata_payload(vertical: &VerticalInvocationOutcome) -> Vec<u8> {
    let metadata = vertical.result_metadata;
    let mut bytes = Vec::with_capacity(V0_METADATA_PAYLOAD_DOMAIN.len() + 1 + 32);
    bytes.extend_from_slice(V0_METADATA_PAYLOAD_DOMAIN);
    bytes.push(0);
    bytes.extend_from_slice(&metadata.stream_id.to_le_bytes());
    bytes.extend_from_slice(&metadata.row_count_exact.unwrap_or_default().to_le_bytes());
    bytes.extend_from_slice(&metadata.column_count.to_le_bytes());
    bytes.push(u8::from(metadata.row_count_exact.is_some()));
    bytes
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

fn emit_v0_outcome_events(
    outcome: &V0InventoryRecoverableOutcome,
    sink: &mut impl EventSink,
) -> AndromedaResult<()> {
    let protocol = ProtocolCorrelation {
        protocol_version: Some(1),
        stream_id: Some(outcome.vertical.result_metadata.stream_id),
        stream_role: Some(StreamRole::ResultUnidirectional as u16),
        frame_type: Some(FrameType::RpcCompletion.wire_code() as u16),
        payload_kind: Some(FrameType::RpcCompletion.wire_code() as u16),
        sequence: Some(3),
    };
    let pre_transaction_correlation = EventCorrelation {
        request_id: Some(outcome.command_frame.header.request_id),
        session_id: Some(outcome.command_frame.header.session_id),
        contract_hash: Some(outcome.srpl_plan.evidence.procedure_contract.contract_hash),
        catalog_version: Some(outcome.srpl_plan.evidence.catalog_version),
        catalog_object_id: Some(outcome.srpl_plan.evidence.procedure_object.object_id),
        transaction_id: None,
        durable_lsn: None,
        protocol: Some(protocol),
    };
    let transaction_id = transaction_id_for_invocation(outcome.vertical.completion.invocation_id);
    let durable_lsn = outcome.vertical.completion.durable_lsn.ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "V0 completion event requires durable LSN evidence",
        )
    })?;
    let completion_correlation = EventCorrelation {
        transaction_id: Some(transaction_id),
        durable_lsn: Some(durable_lsn.get()),
        ..pre_transaction_correlation
    };
    sink.emit(EventEnvelope::new(
        v0_event_id(outcome, 1),
        pre_transaction_correlation,
        TraceEvent::Decision(outcome.vertical.contract_trace.clone()),
    )?)?;
    if let Some(authorization_trace) = &outcome.vertical.authorization_trace {
        sink.emit(EventEnvelope::new(
            v0_event_id(outcome, 2),
            pre_transaction_correlation,
            TraceEvent::Decision(authorization_trace.clone()),
        )?)?;
    }
    sink.emit(EventEnvelope::new(
        v0_event_id(outcome, 3),
        completion_correlation,
        TraceEvent::CompletionEmitted(CompletionEmittedTrace {
            trace_id: outcome.vertical.completion.trace_id,
            protocol,
            completion_code: Some(1),
            committed: true,
            durable_lsn: Some(durable_lsn.get()),
            reason: "V0 Inventory.ReserveStock emitted committed completion after durable WAL"
                .to_string(),
        }),
    )?)?;
    Ok(())
}

fn v0_event_id(outcome: &V0InventoryRecoverableOutcome, suffix: u64) -> EventId {
    EventId::new(u128::from(
        outcome
            .vertical
            .completion
            .invocation_id
            .get()
            .saturating_mul(10)
            .saturating_add(suffix),
    ))
}

fn v0_protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}
