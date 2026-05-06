use crate::{contracts::StatsVersion, digest::Sha256};

use super::{
    HistogramPlaceholder, MAX_HISTOGRAMS_PER_PUBLICATION, STATS_PUBLICATION_DOMAIN,
    StatsColumnTarget, StatsPublicationDigest, StatsValidationError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsPublication {
    version: StatsVersion,
    entries: Vec<(StatsColumnTarget, HistogramPlaceholder)>,
    digest: StatsPublicationDigest,
}

impl StatsPublication {
    pub fn version(&self) -> StatsVersion {
        self.version
    }

    pub fn entries(&self) -> &[(StatsColumnTarget, HistogramPlaceholder)] {
        &self.entries
    }

    pub fn digest(&self) -> StatsPublicationDigest {
        self.digest
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug, Clone)]
pub struct StatsPublicationBuilder {
    version: StatsVersion,
    entries: Vec<(StatsColumnTarget, HistogramPlaceholder)>,
}

impl StatsPublicationBuilder {
    pub fn new(version: StatsVersion) -> Result<Self, StatsValidationError> {
        if version.get() == 0 {
            return Err(StatsValidationError::StatsVersionZero);
        }
        Ok(Self {
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

    // StatsVersion binding, canonical ordering, and digest byte compatibility are invariant.
    pub fn finish(mut self) -> StatsPublication {
        self.entries
            .sort_by_key(|(target, _)| target.canonical_key());

        let mut hasher = Sha256::new();
        hasher.update(STATS_PUBLICATION_DOMAIN);
        hasher.update(&[0xD0]);
        hasher.update(&self.version.get().to_le_bytes());
        hasher.update(&[0xD1]);
        hasher.update(&(self.entries.len() as u32).to_le_bytes());
        for (target, histogram) in &self.entries {
            hasher.update(&[0xD2]);
            hasher.update(&target.object_id.get().to_le_bytes());
            hasher.update(&target.column_index.to_le_bytes());
            histogram.absorb(&mut hasher);
        }

        let digest = StatsPublicationDigest::from_bytes(hasher.finalize());
        StatsPublication {
            version: self.version,
            entries: self.entries,
            digest,
        }
    }
}
