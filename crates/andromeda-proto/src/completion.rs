#[cfg(test)]
mod tests;

use crate::ProtocolVersion;
use andromeda_error::AndromedaResult;
pub use andromeda_procedure_contract::{
    CompletionEnvelopeVersion, CompletionProtocolVersion, CompletionTerminalCode,
    ResultRowCountSummary, RpcCompletion, RpcCompletionStatus, TransactionOutcome,
};

/// Stable completion envelope contract version. Bumped only when the
/// `RpcCompletion` shape, terminal status codes, or transactional binding
/// rules change in a backwards-incompatible way.
pub const COMPLETION_ENVELOPE_VERSION: ProtocolVersion = ProtocolVersion::V1;

impl CompletionProtocolVersion for ProtocolVersion {
    fn validate_completion_protocol_version(self) -> AndromedaResult<()> {
        self.validate()
    }

    fn completion_protocol_major(self) -> u32 {
        self.major
    }

    fn completion_protocol_minor(self) -> u32 {
        self.minor
    }
}
