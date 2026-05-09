/// Errors raised while evaluating runtime-free optimizer plan inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerError {
    PolicyVersionZero,
    TraceIdZero,
    TraceBuildFailed,
}

impl core::fmt::Display for OptimizerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PolicyVersionZero => f.write_str("optimizer policy version must not be zero"),
            Self::TraceIdZero => f.write_str("optimizer trace id must not be zero"),
            Self::TraceBuildFailed => f.write_str("optimizer decision trace construction failed"),
        }
    }
}

impl std::error::Error for OptimizerError {}
