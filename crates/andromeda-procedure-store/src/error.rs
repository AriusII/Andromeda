use std::{error::Error, fmt};

pub type ProcedureStorePrimitiveResult<T> = Result<T, ProcedureStorePrimitiveError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureStorePrimitiveError {
    NonTerminalFeedbackStatus,
    RegressionThresholdOutOfRange,
    ZeroAuditCorrelationId,
    ZeroInvocationId,
    ZeroFeedbackId,
    ZeroProcedureId,
    ZeroEvidenceDigest,
}

impl fmt::Display for ProcedureStorePrimitiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonTerminalFeedbackStatus => {
                f.write_str("invocation feedback status must be terminal")
            },
            Self::RegressionThresholdOutOfRange => {
                f.write_str("regression threshold basis points must be between 0 and 10000")
            },
            Self::ZeroAuditCorrelationId => f.write_str("audit correlation id must not be zero"),
            Self::ZeroInvocationId => f.write_str("procedure invocation id must not be zero"),
            Self::ZeroFeedbackId => f.write_str("procedure feedback id must not be zero"),
            Self::ZeroProcedureId => f.write_str("procedure id must not be zero"),
            Self::ZeroEvidenceDigest => f.write_str("invocation evidence digest must not be zero"),
        }
    }
}

impl Error for ProcedureStorePrimitiveError {}
