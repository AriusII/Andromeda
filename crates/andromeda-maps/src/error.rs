use std::{error::Error, fmt};

pub type MapDescriptorResult<T> = Result<T, MapDescriptorError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapDescriptorError {
    ZeroMapId,
    ZeroBoundedStalenessMillis,
    ZeroCatalogVersion,
    ZeroStatsVersion,
    ZeroPublicationLsn,
    ZeroRollbackLsn,
    ZeroRebuildLsn,
    ZeroRecoveryLsn,
    EmptyValidationDigest,
    EmptyRollbackDigest,
    EmptyRebuildDigest,
    EmptyRecoveryDigest,
    AnalyticsWorkloadNotMapRefresh,
    AnalyticsJobNotAdvisory,
    ColumnarConsumerNotMaps,
    ColumnarArtifactNotAdvisory,
}

impl fmt::Display for MapDescriptorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroMapId => f.write_str("map descriptor id must not be zero"),
            Self::ZeroBoundedStalenessMillis => {
                f.write_str("bounded map staleness must be greater than zero milliseconds")
            },
            Self::ZeroCatalogVersion => {
                f.write_str("map publication catalog version must not be zero")
            },
            Self::ZeroStatsVersion => {
                f.write_str("map publication statistics version must not be zero")
            },
            Self::ZeroPublicationLsn => f.write_str("map publication LSN must not be zero"),
            Self::ZeroRollbackLsn => f.write_str("map rollback LSN must not be zero"),
            Self::ZeroRebuildLsn => f.write_str("map rebuild LSN must not be zero"),
            Self::ZeroRecoveryLsn => f.write_str("map recovery LSN must not be zero"),
            Self::EmptyValidationDigest => {
                f.write_str("map publication validation digest must not be empty")
            },
            Self::EmptyRollbackDigest => f.write_str("map rollback digest must not be empty"),
            Self::EmptyRebuildDigest => f.write_str("map rebuild digest must not be empty"),
            Self::EmptyRecoveryDigest => f.write_str("map recovery digest must not be empty"),
            Self::AnalyticsWorkloadNotMapRefresh => {
                f.write_str("map refresh plan analytics job must use MapRefresh workload")
            },
            Self::AnalyticsJobNotAdvisory => {
                f.write_str("map refresh plan analytics job must remain bounded and advisory")
            },
            Self::ColumnarConsumerNotMaps => {
                f.write_str("map refresh plan columnar layout must target Maps")
            },
            Self::ColumnarArtifactNotAdvisory => {
                f.write_str("map refresh plan columnar layout must remain advisory")
            },
        }
    }
}

impl Error for MapDescriptorError {}
