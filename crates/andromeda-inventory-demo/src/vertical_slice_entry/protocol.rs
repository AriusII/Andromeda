use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_rpc_protocol::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameCodec, FrameHeader, FrameType, StreamRole,
};
use andromeda_types::{RequestId, SessionId};

use crate::ReserveStockCommand;

const V0_RPC_EXECUTE_PAYLOAD_DOMAIN: &[u8] = b"andromeda.exec.v0.inventory-reserve-stock-rpc.v1";
const V0_RPC_EXECUTE_PAYLOAD_LEN: usize = V0_RPC_EXECUTE_PAYLOAD_DOMAIN.len() + 1 + 16;
const V0_RPC_EXECUTE_FRAME_LEN: usize = FrameCodec::HEADER_LEN + V0_RPC_EXECUTE_PAYLOAD_LEN;

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
