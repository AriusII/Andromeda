use andromeda_contract::StatsVersion;
use andromeda_types::CatalogVersion;

use crate::StatisticsError;

/// Stable digest for the bounded set of statistics consumed by a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatsSetDigest([u8; Self::LEN]);

impl StatsSetDigest {
    pub const LEN: usize = 32;

    pub fn new(bytes: [u8; Self::LEN]) -> Result<Self, StatisticsError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(StatisticsError::StatsDigestZero);
        }
        Ok(Self(bytes))
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }
}

/// Publication state for a statistics object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsPublicationState {
    Candidate,
    Validating,
    Published,
    Rejected,
    Expired,
    Superseded,
}

impl StatsPublicationState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Validating => "validating",
            Self::Published => "published",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Superseded => "superseded",
        }
    }

    pub const fn is_optimizer_usable(self) -> bool {
        matches!(self, Self::Published)
    }
}

/// Runtime-free descriptor for a published statistics view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsObjectDescriptor {
    catalog_version: CatalogVersion,
    stats_version: StatsVersion,
    digest: StatsSetDigest,
    state: StatsPublicationState,
    stale: bool,
}

impl StatsObjectDescriptor {
    pub fn new(
        catalog_version: CatalogVersion,
        stats_version: StatsVersion,
        digest: StatsSetDigest,
        state: StatsPublicationState,
    ) -> Result<Self, StatisticsError> {
        if catalog_version.get() == 0 {
            return Err(StatisticsError::CatalogVersionZero);
        }
        if stats_version.is_zero() {
            return Err(StatisticsError::StatsVersionZero);
        }
        Ok(Self {
            catalog_version,
            stats_version,
            digest,
            state,
            stale: false,
        })
    }

    pub const fn mark_stale(mut self) -> Self {
        self.stale = true;
        self
    }

    pub const fn catalog_version(self) -> CatalogVersion {
        self.catalog_version
    }

    pub const fn stats_version(self) -> StatsVersion {
        self.stats_version
    }

    pub const fn digest(self) -> StatsSetDigest {
        self.digest
    }

    pub const fn state(self) -> StatsPublicationState {
        self.state
    }

    pub const fn is_stale(self) -> bool {
        self.stale
    }

    pub const fn is_published(self) -> bool {
        self.state.is_optimizer_usable()
    }

    pub const fn is_versioned(self) -> bool {
        self.catalog_version.get() != 0 && !self.stats_version.is_zero()
    }
}
