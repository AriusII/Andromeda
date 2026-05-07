use andromeda_types::CatalogVersion;

use crate::contracts::StatsVersion;

use super::super::{
    HistogramPlaceholder, MAX_HISTOGRAMS_PER_PUBLICATION, StatsColumnTarget,
    StatsPublicationDigest, StatsValidationError,
};
use super::{digest::compute_publication_digest, validation::validate_histogram};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsPublicationSummary {
    pub catalog_version: CatalogVersion,
    pub version: StatsVersion,
    pub digest: StatsPublicationDigest,
    pub entry_count: usize,
}

impl StatsPublicationSummary {
    pub fn from_publication(publication: &StatsPublication) -> Self {
        Self {
            catalog_version: publication.catalog_version,
            version: publication.version,
            digest: publication.digest,
            entry_count: publication.entries.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsPublication {
    catalog_version: CatalogVersion,
    version: StatsVersion,
    entries: Vec<(StatsColumnTarget, HistogramPlaceholder)>,
    digest: StatsPublicationDigest,
}

impl StatsPublication {
    pub fn catalog_version(&self) -> CatalogVersion {
        self.catalog_version
    }

    pub fn version(&self) -> StatsVersion {
        self.version
    }

    pub fn entries(&self) -> &[(StatsColumnTarget, HistogramPlaceholder)] {
        &self.entries
    }

    pub fn digest(&self) -> StatsPublicationDigest {
        self.digest
    }

    pub fn summary(&self) -> StatsPublicationSummary {
        StatsPublicationSummary::from_publication(self)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn validate(&self) -> Result<(), StatsValidationError> {
        if self.catalog_version.get() == 0 {
            return Err(StatsValidationError::CatalogVersionZero);
        }
        if self.version.get() == 0 {
            return Err(StatsValidationError::StatsVersionZero);
        }
        if self.entries.len() > MAX_HISTOGRAMS_PER_PUBLICATION {
            return Err(StatsValidationError::PublicationExceedsHistogramCap);
        }

        let mut previous_key: Option<(u64, u16)> = None;
        for (target, histogram) in &self.entries {
            if target.object_id.get() == 0 {
                return Err(StatsValidationError::ZeroObjectId);
            }
            let key = target.canonical_key();
            if let Some(previous) = previous_key {
                if previous == key {
                    return Err(StatsValidationError::DuplicateTarget);
                }
                if previous > key {
                    return Err(StatsValidationError::PublicationTargetsNotCanonical);
                }
            }
            validate_histogram(histogram)?;
            previous_key = Some(key);
        }

        let expected =
            compute_publication_digest(self.catalog_version, self.version, &self.entries);
        if expected != self.digest {
            return Err(StatsValidationError::PublicationDigestMismatch);
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct StatsPublicationBuilder {
    catalog_version: CatalogVersion,
    version: StatsVersion,
    entries: Vec<(StatsColumnTarget, HistogramPlaceholder)>,
}

impl StatsPublicationBuilder {
    pub fn new(
        catalog_version: CatalogVersion,
        version: StatsVersion,
    ) -> Result<Self, StatsValidationError> {
        if catalog_version.get() == 0 {
            return Err(StatsValidationError::CatalogVersionZero);
        }
        if version.get() == 0 {
            return Err(StatsValidationError::StatsVersionZero);
        }
        Ok(Self {
            catalog_version,
            version,
            entries: Vec::new(),
        })
    }

    pub fn push(
        mut self,
        target: StatsColumnTarget,
        histogram: HistogramPlaceholder,
    ) -> Result<Self, StatsValidationError> {
        if target.object_id.get() == 0 {
            return Err(StatsValidationError::ZeroObjectId);
        }
        if self.entries.len() >= MAX_HISTOGRAMS_PER_PUBLICATION {
            return Err(StatsValidationError::PublicationExceedsHistogramCap);
        }
        if self.entries.iter().any(|(existing, _)| *existing == target) {
            return Err(StatsValidationError::DuplicateTarget);
        }
        self.entries.push((target, histogram));
        Ok(self)
    }

    pub fn finish(mut self) -> StatsPublication {
        self.entries
            .sort_by_key(|(target, _)| target.canonical_key());
        let digest = compute_publication_digest(self.catalog_version, self.version, &self.entries);
        StatsPublication {
            catalog_version: self.catalog_version,
            version: self.version,
            entries: self.entries,
            digest,
        }
    }
}
