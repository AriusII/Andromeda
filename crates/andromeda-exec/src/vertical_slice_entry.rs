use andromeda_catalog::{CatalogSnapshot, ProcedureContract};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId, TransactionId,
};
use andromeda_observe::{
    CompletionEmittedTrace, EventCorrelation, EventEmitter, EventSink, ProtocolCorrelation,
    TraceEvent,
};
use andromeda_quic::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameCodec, FrameHeader, FrameType,
    ResultStreamMetadataPolicy, StreamRole, validate_result_stream_sequence_with_metadata_policy,
};
use andromeda_srpl::{
    ExecutableProcedurePlan, bind_executable_procedure_plan, compile_narrow_procedure_signature,
};
use andromeda_storage::{Lsn, PageId, PageSize};

use crate::{
    CompletionStatus, InventoryProductStockCommitEvidence,
    InventoryProductStockDurableRedoEvidence, InventoryProductStockStore, InventoryStock,
    InvocationContext, InvocationReject, InvocationRequest, InvocationWal, LocalVerticalRuntime,
    ReserveStockCommand, ReserveStockEffect, VerticalInvocationOutcome,
    business::HeapInventoryProductStockStore,
};

const V0_RPC_EXECUTE_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.inventory-reserve-stock-rpc.v1";
const V0_METADATA_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.result-metadata.v1";
const V0_BATCH_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.inventory-reservation-batch.v1";
const V0_COMPLETION_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.completion.v1";
const V0_RPC_EXECUTE_PAYLOAD_LEN: usize = V0_RPC_EXECUTE_PAYLOAD_DOMAIN.len() + 1 + 16;
const V0_RPC_EXECUTE_FRAME_LEN: usize = FrameCodec::HEADER_LEN + V0_RPC_EXECUTE_PAYLOAD_LEN;
const V0_INVENTORY_RESERVE_STOCK_CONTRACT_KIND: u16 = 1;
const V0_PRODUCT_STOCK_HEAP_PAGE_ID: PageId = PageId::new(42_000);
const V0_PRODUCT_STOCK_HEAP_PAGE_SIZE: PageSize = PageSize::KiB16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V0InventoryProtocolViolation {
    ExecuteFrameByteLength { expected: usize, actual: usize },
    ExecuteFrameDecodeRejected { reason: String },
    ExecuteFrameTransportRejected { reason: String },
    ExecuteFrameType { actual: FrameType },
    ExecuteFrameCarriesTransaction,
    RpcPayloadByteLength { expected: usize, actual: usize },
    RpcPayloadDomain,
    RpcPayloadFieldTruncated { field: &'static str },
    RpcPayloadFieldOffsetOverflow { field: &'static str },
}

impl V0InventoryProtocolViolation {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ExecuteFrameByteLength { .. } => "AE-V0-RSVSTK-FRAME-LEN",
            Self::ExecuteFrameDecodeRejected { .. } => "AE-V0-RSVSTK-FRAME-DECODE",
            Self::ExecuteFrameTransportRejected { .. } => "AE-V0-RSVSTK-FRAME-STREAM",
            Self::ExecuteFrameType { .. } => "AE-V0-RSVSTK-FRAME-TYPE",
            Self::ExecuteFrameCarriesTransaction => "AE-V0-RSVSTK-FRAME-TXID",
            Self::RpcPayloadByteLength { .. } => "AE-V0-RSVSTK-PAYLOAD-LEN",
            Self::RpcPayloadDomain => "AE-V0-RSVSTK-PAYLOAD-DOMAIN",
            Self::RpcPayloadFieldTruncated { .. } => "AE-V0-RSVSTK-PAYLOAD-FIELD",
            Self::RpcPayloadFieldOffsetOverflow { .. } => "AE-V0-RSVSTK-PAYLOAD-OFFSET",
        }
    }

    fn message(&self) -> String {
        match self {
            Self::ExecuteFrameByteLength { expected, actual } => format!(
                "V0 Inventory.ReserveStock protocol rejection [{}]: execute frame byte length {actual} does not match fixed bounded length {expected}",
                self.code()
            ),
            Self::ExecuteFrameDecodeRejected { reason } => format!(
                "V0 Inventory.ReserveStock protocol rejection [{}]: execute frame decode rejected: {reason}",
                self.code()
            ),
            Self::ExecuteFrameTransportRejected { reason } => format!(
                "V0 Inventory.ReserveStock protocol rejection [{}]: execute frame stream policy rejected: {reason}",
                self.code()
            ),
            Self::ExecuteFrameType { actual } => format!(
                "V0 Inventory.ReserveStock protocol rejection [{}]: execute frame type must be RpcExecuteRequest, got {actual:?}",
                self.code()
            ),
            Self::ExecuteFrameCarriesTransaction => format!(
                "V0 Inventory.ReserveStock protocol rejection [{}]: execute frame must be pre-transaction and omit transaction evidence",
                self.code()
            ),
            Self::RpcPayloadByteLength { expected, actual } => format!(
                "V0 Inventory.ReserveStock protocol rejection [{}]: RPC payload byte length {actual} does not match fixed bounded length {expected}",
                self.code()
            ),
            Self::RpcPayloadDomain => format!(
                "V0 Inventory.ReserveStock protocol rejection [{}]: RPC payload domain tag mismatch",
                self.code()
            ),
            Self::RpcPayloadFieldTruncated { field } => format!(
                "V0 Inventory.ReserveStock protocol rejection [{}]: RPC payload field '{field}' is truncated",
                self.code()
            ),
            Self::RpcPayloadFieldOffsetOverflow { field } => format!(
                "V0 Inventory.ReserveStock protocol rejection [{}]: RPC payload field '{field}' offset overflow",
                self.code()
            ),
        }
    }
}

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
        let expected_len = V0_RPC_EXECUTE_PAYLOAD_LEN;
        if bytes.len() != expected_len {
            return Err(v0_protocol_error(
                V0InventoryProtocolViolation::RpcPayloadByteLength {
                    expected: expected_len,
                    actual: bytes.len(),
                },
            ));
        }
        if !bytes.starts_with(V0_RPC_EXECUTE_PAYLOAD_DOMAIN)
            || bytes[V0_RPC_EXECUTE_PAYLOAD_DOMAIN.len()] != 0
        {
            return Err(v0_protocol_error(
                V0InventoryProtocolViolation::RpcPayloadDomain,
            ));
        }

        let data = &bytes[V0_RPC_EXECUTE_PAYLOAD_DOMAIN.len() + 1..];
        let product_id = decode_v0_i64_field(data, 0, "product id")?;
        let quantity = decode_v0_i64_field(data, 8, "quantity")?;
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
    pub product_stock_commit: InventoryProductStockCommitEvidence,
    pub product_stock_redo: Option<InventoryProductStockDurableRedoEvidence>,
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
    pub fn new(wal: W) -> Self {
        Self {
            local: LocalVerticalRuntime::new(wal),
        }
    }

    /// Wrap an externally constructed [`LocalVerticalRuntime`] so callers can
    /// inject a recovery-seeded [`andromeda_tx::TransactionManager`] before
    /// the V0 path begins serving traffic.
    pub fn from_local_runtime(local: LocalVerticalRuntime<W>) -> Self {
        Self { local }
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

    #[allow(
        clippy::too_many_arguments,
        reason = "V0 vertical slice keeps each contractual input explicit at the RPC boundary."
    )]
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
        let mut product_stock = v0_product_stock_store_from_cold_snapshot(observed_stock)?;
        self.execute_encoded_inventory_reserve_stock_with_product_stock(
            encoded_execute_frame,
            srpl_source,
            catalog,
            contract,
            request,
            context,
            &mut product_stock,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "V0 vertical slice keeps each contractual input explicit at the RPC boundary."
    )]
    pub fn execute_encoded_inventory_reserve_stock_with_product_stock<P>(
        &mut self,
        encoded_execute_frame: &[u8],
        srpl_source: &str,
        catalog: &CatalogSnapshot,
        contract: &ProcedureContract,
        request: InvocationRequest,
        context: &InvocationContext,
        product_stock: &mut P,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome>
    where
        P: InventoryProductStockStore,
    {
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

        validate_v0_reserve_stock_before_product_stock_prepare(&request, context, contract)?;

        let prepared = product_stock.prepare_reserve_stock(command)?;
        let product_stock_redo_template =
            product_stock.prepared_reserve_stock_redo_template(&prepared)?;
        let effect = prepared.effect.clone();
        let mut local_procedure = effect.to_local_procedure(contract)?;
        if let Some(template) = &product_stock_redo_template {
            local_procedure.mutation_payload = template.encode_template()?;
        }
        let vertical = match self
            .local
            .execute_authorized(request, &local_procedure, context)
        {
            Ok(vertical) => vertical,
            Err(error) => {
                product_stock.abort_prepared_reserve_stock(&prepared, error.message())?;
                return Err(error);
            }
        };
        let durable_lsn = vertical.completion.durable_lsn.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "V0 ProductStock publication requires durable commit LSN evidence",
            )
        })?;
        let product_stock_commit =
            InventoryProductStockCommitEvidence::new(vertical.transaction_id, durable_lsn)?;
        let product_stock_redo = if let Some(template) = product_stock_redo_template {
            let wal_evidence = vertical.wal_evidence.ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "V0 ProductStock publication requires local WAL evidence for storage redo",
                )
            })?;
            let redo_record_lsn = wal_evidence.mutation_lsn.ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "V0 ProductStock publication requires a durable row redo WAL record LSN",
                )
            })?;
            let redo_payload = template.materialize_heap_redo_payload(redo_record_lsn)?;
            let redo = InventoryProductStockDurableRedoEvidence::new(
                vertical.transaction_id,
                durable_lsn,
                redo_payload,
            )?;
            product_stock.publish_committed_reserve_stock_with_redo(
                &prepared,
                product_stock_commit,
                redo.clone(),
            )?;
            Some(redo)
        } else {
            product_stock.publish_committed_reserve_stock(&prepared, product_stock_commit)?;
            None
        };
        let tx_id = vertical
            .completion
            .transaction_state
            .map(|_| vertical.transaction_id);
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
            product_stock_commit,
            product_stock_redo,
            vertical,
            command_frame,
            result_frames,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Observed V0 path keeps transport, catalog, contract, request, and stock evidence separate."
    )]
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
        let mut emitter = EventEmitter::new(sink);
        self.execute_encoded_inventory_reserve_stock_observed_with_emitter(
            encoded_execute_frame,
            srpl_source,
            catalog,
            contract,
            request,
            context,
            observed_stock,
            &mut emitter,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Observed V0 path keeps transport, catalog, contract, request, stock adapter, and sink evidence separate."
    )]
    pub fn execute_encoded_inventory_reserve_stock_observed_with_product_stock<P>(
        &mut self,
        encoded_execute_frame: &[u8],
        srpl_source: &str,
        catalog: &CatalogSnapshot,
        contract: &ProcedureContract,
        request: InvocationRequest,
        context: &InvocationContext,
        product_stock: &mut P,
        sink: &mut impl EventSink,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome>
    where
        P: InventoryProductStockStore,
    {
        let mut emitter = EventEmitter::new(sink);
        self.execute_encoded_inventory_reserve_stock_observed_with_product_stock_and_emitter(
            encoded_execute_frame,
            srpl_source,
            catalog,
            contract,
            request,
            context,
            product_stock,
            &mut emitter,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Observed V0 path keeps transport, catalog, contract, request, and emitter evidence separate."
    )]
    pub fn execute_encoded_inventory_reserve_stock_observed_with_emitter<S: EventSink>(
        &mut self,
        encoded_execute_frame: &[u8],
        srpl_source: &str,
        catalog: &CatalogSnapshot,
        contract: &ProcedureContract,
        request: InvocationRequest,
        context: &InvocationContext,
        observed_stock: InventoryStock,
        emitter: &mut EventEmitter<S>,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome> {
        let mut product_stock = v0_product_stock_store_from_cold_snapshot(observed_stock)?;
        self.execute_encoded_inventory_reserve_stock_observed_with_product_stock_and_emitter(
            encoded_execute_frame,
            srpl_source,
            catalog,
            contract,
            request,
            context,
            &mut product_stock,
            emitter,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Observed V0 path keeps transport, catalog, contract, request, stock adapter, and emitter evidence separate."
    )]
    pub fn execute_encoded_inventory_reserve_stock_observed_with_product_stock_and_emitter<
        P,
        S: EventSink,
    >(
        &mut self,
        encoded_execute_frame: &[u8],
        srpl_source: &str,
        catalog: &CatalogSnapshot,
        contract: &ProcedureContract,
        request: InvocationRequest,
        context: &InvocationContext,
        product_stock: &mut P,
        emitter: &mut EventEmitter<S>,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome>
    where
        P: InventoryProductStockStore,
    {
        let rejection_request = request.clone();
        let rejection_context = context.clone();
        let outcome = match self.execute_encoded_inventory_reserve_stock_with_product_stock(
            encoded_execute_frame,
            srpl_source,
            catalog,
            contract,
            request,
            context,
            product_stock,
        ) {
            Ok(outcome) => outcome,
            Err(error) => {
                if let Some(reject) = v0_pre_transaction_reject_from_error(&error) {
                    let command_frame = decode_v0_execute_frame(encoded_execute_frame)?;
                    emit_v0_inventory_reserve_stock_pre_transaction_refusal(
                        &rejection_request,
                        &rejection_context,
                        command_frame.header.request_id,
                        command_frame.header.session_id,
                        contract,
                        &reject,
                        emitter,
                    )?;
                }
                return Err(error);
            }
        };

        emit_v0_outcome_events(&outcome, emitter)?;
        Ok(outcome)
    }
}

fn v0_product_stock_store_from_cold_snapshot(
    observed_stock: InventoryStock,
) -> AndromedaResult<HeapInventoryProductStockStore> {
    HeapInventoryProductStockStore::from_cold_snapshot(
        V0_PRODUCT_STOCK_HEAP_PAGE_ID,
        V0_PRODUCT_STOCK_HEAP_PAGE_SIZE,
        observed_stock,
    )
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
    if encoded_execute_frame.len() != V0_RPC_EXECUTE_FRAME_LEN {
        return Err(v0_protocol_error(
            V0InventoryProtocolViolation::ExecuteFrameByteLength {
                expected: V0_RPC_EXECUTE_FRAME_LEN,
                actual: encoded_execute_frame.len(),
            },
        ));
    }

    let frame = FrameCodec::decode(encoded_execute_frame).map_err(|error| {
        v0_protocol_error(V0InventoryProtocolViolation::ExecuteFrameDecodeRejected {
            reason: error.message().to_string(),
        })
    })?;
    frame
        .validate(StreamRole::CommandBidirectional)
        .map_err(|error| {
            v0_protocol_error(
                V0InventoryProtocolViolation::ExecuteFrameTransportRejected {
                    reason: error.message().to_string(),
                },
            )
        })?;
    if frame.header.frame_type != FrameType::RpcExecuteRequest {
        return Err(v0_protocol_error(
            V0InventoryProtocolViolation::ExecuteFrameType {
                actual: frame.header.frame_type,
            },
        ));
    }
    if frame.header.tx_id.is_some() {
        return Err(v0_protocol_error(
            V0InventoryProtocolViolation::ExecuteFrameCarriesTransaction,
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

fn emit_v0_outcome_events<S: EventSink>(
    outcome: &V0InventoryRecoverableOutcome,
    emitter: &mut EventEmitter<S>,
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
    let transaction_id = outcome.vertical.transaction_id;
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
    emitter.emit(
        pre_transaction_correlation,
        TraceEvent::Decision(outcome.vertical.admission_trace.clone()),
    )?;
    emitter.emit(
        pre_transaction_correlation,
        TraceEvent::Decision(outcome.vertical.contract_trace.clone()),
    )?;
    if let Some(authorization_trace) = &outcome.vertical.authorization_trace {
        emitter.emit(
            pre_transaction_correlation,
            TraceEvent::Decision(authorization_trace.clone()),
        )?;
    }
    emitter.emit(
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
    )?;
    Ok(())
}

/// Emit typed evidence for V0 invocation refusals that happen before a
/// transaction exists.
///
/// This helper is intentionally limited to the C4 pre-transaction surface:
/// contract/admission rejections and authorization denials. It never fabricates
/// transaction or durable-LSN correlation, and it lets [`EventEmitter`] surface
/// sink/envelope failures to the caller.
pub fn emit_v0_inventory_reserve_stock_pre_transaction_refusal<S: EventSink>(
    request: &InvocationRequest,
    context: &InvocationContext,
    request_id: RequestId,
    session_id: SessionId,
    contract: &ProcedureContract,
    reject: &InvocationReject,
    emitter: &mut EventEmitter<S>,
) -> AndromedaResult<()> {
    let protocol = v0_execute_request_protocol();
    let correlation = EventCorrelation {
        request_id: Some(request_id),
        session_id: Some(session_id),
        contract_hash: Some(request.expected_contract_hash),
        catalog_version: Some(request.catalog_version),
        catalog_object_id: Some(contract.object.object_id),
        transaction_id: None,
        durable_lsn: None,
        protocol: Some(protocol),
    };

    match reject.status {
        CompletionStatus::ContractRejected | CompletionStatus::FailedBeforeTransaction => {
            if let Some(trace) = reject.contract_rejected_trace(
                context.trace_id,
                protocol,
                V0_INVENTORY_RESERVE_STOCK_CONTRACT_KIND,
                reject.status.terminal_code() as u16,
            ) {
                emitter.emit(correlation, TraceEvent::ContractRejected(trace))?;
            }
        }
        CompletionStatus::PermissionDenied => {
            if let Some(trace) = reject.authorization_denial_trace(
                context.trace_id,
                denied_permission_for_context(contract, context),
            ) {
                emitter.emit(correlation, TraceEvent::AuthorizationDenied(trace))?;
            }
        }
        CompletionStatus::SystemUnavailable
        | CompletionStatus::Cancelled
        | CompletionStatus::Poisoned
        | CompletionStatus::Committed
        | CompletionStatus::RolledBack => {}
    }

    if request.invocation_id.get() != 0 {
        emitter.emit(
            correlation,
            TraceEvent::ExecutionTransition(reject.project_transition(
                request.invocation_id,
                context.trace_id,
                Some(request_id),
                Some(session_id),
            )),
        )?;
    }

    Ok(())
}

fn v0_pre_transaction_reject_from_error(error: &AndromedaError) -> Option<InvocationReject> {
    let status = match error.kind() {
        AndromedaErrorKind::Contract => CompletionStatus::ContractRejected,
        AndromedaErrorKind::Security => CompletionStatus::PermissionDenied,
        _ => return None,
    };

    Some(InvocationReject {
        status,
        reason: error.message().to_string(),
    })
}

fn validate_v0_reserve_stock_before_product_stock_prepare(
    request: &InvocationRequest,
    context: &InvocationContext,
    contract: &ProcedureContract,
) -> AndromedaResult<()> {
    request
        .validate_admission(context.trace_id)
        .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
    request
        .validate_before_transaction(contract.binding(), context.trace_id)
        .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
    context
        .authorize(&contract.required_permissions)
        .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Security, reject.reason))?;
    if !request.structured_parameters.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "V0 Inventory.ReserveStock fixed RPC payload path rejects separate StructuredObject parameters before ProductStock preparation",
        ));
    }
    Ok(())
}

fn v0_execute_request_protocol() -> ProtocolCorrelation {
    ProtocolCorrelation {
        protocol_version: Some(1),
        stream_id: None,
        stream_role: Some(StreamRole::CommandBidirectional as u16),
        frame_type: Some(FrameType::RpcExecuteRequest.wire_code() as u16),
        payload_kind: Some(FrameType::RpcExecuteRequest.wire_code() as u16),
        sequence: Some(1),
    }
}

fn denied_permission_for_context(
    contract: &ProcedureContract,
    context: &InvocationContext,
) -> String {
    contract
        .required_permissions
        .iter()
        .find(|permission| !context.grants(permission))
        .cloned()
        .unwrap_or_else(|| "unknown required permission".to_string())
}

fn decode_v0_i64_field(data: &[u8], offset: usize, field: &str) -> AndromedaResult<i64> {
    let end = offset.checked_add(8).ok_or_else(|| {
        v0_protocol_error(
            V0InventoryProtocolViolation::RpcPayloadFieldOffsetOverflow {
                field: stable_v0_payload_field(field),
            },
        )
    })?;
    let bytes = data.get(offset..end).ok_or_else(|| {
        v0_protocol_error(V0InventoryProtocolViolation::RpcPayloadFieldTruncated {
            field: stable_v0_payload_field(field),
        })
    })?;
    let mut value = [0_u8; 8];
    value.copy_from_slice(bytes);
    Ok(i64::from_le_bytes(value))
}

fn stable_v0_payload_field(field: &str) -> &'static str {
    match field {
        "product id" => "product_id",
        "quantity" => "quantity",
        _ => "unknown",
    }
}

fn v0_protocol_error(violation: V0InventoryProtocolViolation) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, violation.message())
}
