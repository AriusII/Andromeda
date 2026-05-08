#[cfg(test)]
mod tests;

use crate::ProtocolVersion;
pub use andromeda_procedure_contract::{
    CompletionEnvelopeVersion, CompletionProtocolVersion, CompletionTerminalCode,
    ResultRowCountSummary, RpcCompletion, RpcCompletionStatus, TransactionOutcome,
};

/// Stable completion envelope contract version. Bumped only when the
/// `RpcCompletion` shape, terminal status codes, or transactional binding
/// rules change in a backwards-incompatible way.
pub const COMPLETION_ENVELOPE_VERSION: ProtocolVersion = ProtocolVersion::V1;
