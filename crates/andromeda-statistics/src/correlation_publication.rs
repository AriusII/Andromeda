use andromeda_types::CatalogVersion;

use andromeda_contract::StatsVersion;
use andromeda_digest::Sha256;

use super::{
    STATS_CORRELATION_PUBLICATION_DOMAIN, StatsCorrelation, StatsPublicationDigest,
    StatsValidationError,
};

pub const MAX_CORRELATIONS_PER_PUBLICATION: usize = 1_024;

/// Bounded, advisory correlation evidence for one catalog/statistics scope.
///
/// Map analytics validators may use this publication for staleness and
/// summarizability checks only after the surrounding stats publication lifecycle
/// has proven durable publication. The publication itself is never source truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsCorrelationPublication {
    catalog_version: CatalogVersion,
    stats_version: StatsVersion,
    entries: Vec<StatsCorrelation>,
    digest: StatsPublicationDigest,
}

impl StatsCorrelationPublication {
    pub const fn catalog_version(&self) -> CatalogVersion {
        self.catalog_version
    }

    pub const fn stats_version(&self) -> StatsVersion {
        self.stats_version
    }

    pub fn entries(&self) -> &[StatsCorrelation] {
        &self.entries
    }

    pub const fn digest(&self) -> StatsPublicationDigest {
        self.digest
    }

    pub const fn is_authoritative(&self) -> bool {
        false
    }

    pub const fn requires_durable_publication_evidence(&self) -> bool {
        true
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_current_for(
        &self,
        catalog_version: CatalogVersion,
        stats_version: StatsVersion,
    ) -> bool {
        self.catalog_version == catalog_version
            && self.stats_version == stats_version
            && self
                .entries
                .iter()
                .all(|entry| entry.is_valid_for(catalog_version, stats_version))
    }

    pub fn is_stale_for(
        &self,
        catalog_version: CatalogVersion,
        stats_version: StatsVersion,
    ) -> bool {
        !self.is_current_for(catalog_version, stats_version)
    }
}

#[derive(Debug, Clone)]
pub struct StatsCorrelationPublicationBuilder {
    catalog_version: CatalogVersion,
    stats_version: StatsVersion,
    entries: Vec<StatsCorrelation>,
}

impl StatsCorrelationPublicationBuilder {
    pub fn new(
        catalog_version: CatalogVersion,
        stats_version: StatsVersion,
    ) -> Result<Self, StatsValidationError> {
        if catalog_version.get() == 0 {
            return Err(StatsValidationError::CatalogVersionZero);
        }
        if stats_version.get() == 0 {
            return Err(StatsValidationError::StatsVersionZero);
        }
        Ok(Self {
            catalog_version,
            stats_version,
            entries: Vec::new(),
        })
    }

    pub fn push(mut self, entry: StatsCorrelation) -> Result<Self, StatsValidationError> {
        if !entry.is_valid_for(self.catalog_version, self.stats_version) {
            return Err(StatsValidationError::CorrelationVersionMismatch);
        }
        if self.entries.len() >= MAX_CORRELATIONS_PER_PUBLICATION {
            return Err(StatsValidationError::CorrelationPublicationExceedsCap);
        }
        if self
            .entries
            .iter()
            .any(|existing| existing.id() == entry.id())
        {
            return Err(StatsValidationError::DuplicateCorrelationId);
        }
        self.entries.push(entry);
        Ok(self)
    }

    // Digest byte order/tags are compatibility-critical.
    pub fn finish(mut self) -> StatsCorrelationPublication {
        self.entries.sort_by_key(|entry| entry.id());

        let mut hasher = Sha256::new();
        hasher.update(STATS_CORRELATION_PUBLICATION_DOMAIN);
        hasher.update(&[0xEA]);
        hasher.update(&self.catalog_version.get().to_le_bytes());
        hasher.update(&[0xEB]);
        hasher.update(&self.stats_version.get().to_le_bytes());
        hasher.update(&[0xEC]);
        hasher.update(&(self.entries.len() as u32).to_le_bytes());
        for entry in &self.entries {
            hasher.update(&[0xED]);
            hasher.update(&entry.digest().as_bytes());
        }

        StatsCorrelationPublication {
            catalog_version: self.catalog_version,
            stats_version: self.stats_version,
            entries: self.entries,
            digest: StatsPublicationDigest::from_bytes(hasher.finalize()),
        }
    }
}
