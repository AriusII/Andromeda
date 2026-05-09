/// Errors raised by statistics descriptors and optimizer-use gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatisticsError {
    CatalogVersionZero,
    StatsVersionZero,
    StatsDigestZero,
    PolicyVersionZero,
    TraceIdZero,
    TraceBuildFailed,
}

impl core::fmt::Display for StatisticsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CatalogVersionZero => f.write_str("statistics catalog version must not be zero"),
            Self::StatsVersionZero => f.write_str("statistics version must not be zero"),
            Self::StatsDigestZero => f.write_str("statistics digest must not be all zero"),
            Self::PolicyVersionZero => f.write_str("statistics policy version must not be zero"),
            Self::TraceIdZero => f.write_str("statistics decision trace id must not be zero"),
            Self::TraceBuildFailed => f.write_str("statistics decision trace construction failed"),
        }
    }
}

impl std::error::Error for StatisticsError {}
