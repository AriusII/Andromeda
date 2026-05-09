//! RPC completion and transaction outcome contract types.
//!
//! This module defines the completion envelope which concludes an RPC stream,
//! including transaction outcome, row counts, and durability evidence.

mod outcome;
mod row_count;
mod status;
mod validation;

#[cfg(test)]
mod tests;

use andromeda_types::{RequestId, SessionId, TransactionId};

pub use outcome::{TRANSACTION_OUTCOME_TERMINAL_CODES, TransactionOutcome};
pub use row_count::ResultRowCountSummary;
pub use status::{
    CompletionTerminalCode, RPC_COMPLETION_STATUS_TERMINAL_CODES, RpcCompletionStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcCompletion {
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
    pub trace_id: Option<String>,
    pub status: RpcCompletionStatus,
    pub transaction_outcome: TransactionOutcome,
    pub rows_affected: Option<u64>,
    pub result_row_counts: Vec<ResultRowCountSummary>,
    pub tx_id: Option<TransactionId>,
    pub durable_lsn: Option<u64>,
}

/// Runtime-free completion envelope version.
///
/// Protocol crates adapt their negotiated wire version to this type through
/// [`CompletionProtocolVersion`]. The completion contract must not depend on a
/// concrete Protobuf or transport crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionEnvelopeVersion {
    pub major: u32,
    pub minor: u32,
}

impl CompletionEnvelopeVersion {
    pub const V1: Self = Self { major: 1, minor: 0 };

    pub const fn is_compatible_with_protocol(self, major: u32, minor: u32) -> bool {
        self.major == major && minor <= self.minor
    }
}

/// Stable completion envelope contract version. Bumped only when the
/// `RpcCompletion` shape, terminal status codes, or transactional binding
/// rules change in a backwards-incompatible way.
pub const COMPLETION_ENVELOPE_CONTRACT_VERSION: CompletionEnvelopeVersion =
    CompletionEnvelopeVersion::V1;

/// Adapter trait for protocol crates that need to validate an RPC completion
/// against a negotiated wire protocol version without coupling this contract
/// crate to a specific Protobuf `ProtocolVersion` type.
pub trait CompletionProtocolVersion: Copy {
    fn validate_completion_protocol_version(self) -> andromeda_error::AndromedaResult<()>;
    fn completion_protocol_major(self) -> u32;
    fn completion_protocol_minor(self) -> u32;
}
