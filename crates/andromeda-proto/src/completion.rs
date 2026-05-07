//! RPC completion and transaction outcome types.
//!
//! This module defines the completion envelope which concludes an RPC stream,
//! including transaction outcome, row counts, and durability evidence.

mod outcome;
mod row_count;
mod status;
mod validation;

#[cfg(test)]
mod tests;

use andromeda_core::{RequestId, SessionId, TransactionId};

use crate::ProtocolVersion;

pub use outcome::TransactionOutcome;
pub use row_count::ResultRowCountSummary;
pub use status::{CompletionTerminalCode, RpcCompletionStatus};

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

/// Stable completion envelope contract version. Bumped only when the
/// `RpcCompletion` shape, terminal status codes, or transactional binding
/// rules change in a backwards-incompatible way.
pub const COMPLETION_ENVELOPE_VERSION: ProtocolVersion = ProtocolVersion::V1;
