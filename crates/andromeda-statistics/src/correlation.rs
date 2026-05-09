use andromeda_types::CatalogVersion;

use andromeda_contract::StatsVersion;
use andromeda_digest::Sha256;

use super::{
    STATS_CORRELATION_DOMAIN, StatsColumnTarget, StatsCorrelationDigest, StatsValidationError,
};

pub const MAX_COLUMNS_PER_CORRELATION: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatsCorrelationId(u64);

impl StatsCorrelationId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StatsCorrelationKind {
    Positive,
    Negative,
    FunctionalDependency,
    JoinKeyEquivalence,
    CoOccurrence,
}

impl StatsCorrelationKind {
    pub const VARIANT_COUNT: usize = 5;

    pub const fn as_tag(self) -> u8 {
        match self {
            StatsCorrelationKind::Positive => 0x41,
            StatsCorrelationKind::Negative => 0x42,
            StatsCorrelationKind::FunctionalDependency => 0x43,
            StatsCorrelationKind::JoinKeyEquivalence => 0x44,
            StatsCorrelationKind::CoOccurrence => 0x45,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CorrelationStrengthPermille(u16);

impl CorrelationStrengthPermille {
    pub const MAX_RAW: u16 = 1_000;

    pub const fn from_permille(value: u16) -> Result<Self, StatsValidationError> {
        if value > Self::MAX_RAW {
            Err(StatsValidationError::CorrelationStrengthOutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn permille(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CorrelationEvidenceBounds {
    pub sample_rows: u64,
    pub population_lower_bound: u64,
    pub population_upper_bound: u64,
    pub confidence_permille: u16,
}

impl CorrelationEvidenceBounds {
    pub fn validate(self) -> Result<(), StatsValidationError> {
        if self.sample_rows == 0 {
            return Err(StatsValidationError::CorrelationSampleRowsZero);
        }
        if self.population_lower_bound > self.population_upper_bound {
            return Err(StatsValidationError::CorrelationPopulationBoundsInverted);
        }
        if self.sample_rows > self.population_upper_bound {
            return Err(StatsValidationError::CorrelationSampleExceedsPopulationUpper);
        }
        if self.confidence_permille > CorrelationStrengthPermille::MAX_RAW {
            return Err(StatsValidationError::CorrelationConfidenceOutOfRange);
        }
        Ok(())
    }

    fn absorb(self, hasher: &mut Sha256) {
        hasher.update(&[0xE8]);
        hasher.update(&self.sample_rows.to_le_bytes());
        hasher.update(&self.population_lower_bound.to_le_bytes());
        hasher.update(&self.population_upper_bound.to_le_bytes());
        hasher.update(&self.confidence_permille.to_le_bytes());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsCorrelation {
    id: StatsCorrelationId,
    catalog_version: CatalogVersion,
    stats_version: StatsVersion,
    kind: StatsCorrelationKind,
    strength: CorrelationStrengthPermille,
    columns: Vec<StatsColumnTarget>,
    evidence_bounds: CorrelationEvidenceBounds,
    digest: StatsCorrelationDigest,
}

impl StatsCorrelation {
    pub fn new(
        id: StatsCorrelationId,
        catalog_version: CatalogVersion,
        stats_version: StatsVersion,
        kind: StatsCorrelationKind,
        strength: CorrelationStrengthPermille,
        columns: Vec<StatsColumnTarget>,
        evidence_bounds: CorrelationEvidenceBounds,
    ) -> Result<Self, StatsValidationError> {
        if catalog_version.get() == 0 {
            return Err(StatsValidationError::CatalogVersionZero);
        }
        if stats_version.get() == 0 {
            return Err(StatsValidationError::StatsVersionZero);
        }
        if columns.len() < 2 {
            return Err(StatsValidationError::CorrelationTooFewColumns);
        }
        if columns.len() > MAX_COLUMNS_PER_CORRELATION {
            return Err(StatsValidationError::CorrelationExceedsColumnCap);
        }

        let mut columns = columns;
        columns.sort_by_key(|column| column.canonical_key());

        let mut previous: Option<StatsColumnTarget> = None;
        for column in &columns {
            if column.object_id.get() == 0 {
                return Err(StatsValidationError::ZeroObjectId);
            }
            if let Some(prev) = previous
                && prev == *column
            {
                return Err(StatsValidationError::CorrelationDuplicateColumn);
            }
            previous = Some(*column);
        }
        evidence_bounds.validate()?;

        let digest = Self::compute_digest(
            id,
            catalog_version,
            stats_version,
            kind,
            strength,
            &columns,
            evidence_bounds,
        );

        Ok(Self {
            id,
            catalog_version,
            stats_version,
            kind,
            strength,
            columns,
            evidence_bounds,
            digest,
        })
    }

    pub const fn id(&self) -> StatsCorrelationId {
        self.id
    }

    pub const fn catalog_version(&self) -> CatalogVersion {
        self.catalog_version
    }

    pub const fn stats_version(&self) -> StatsVersion {
        self.stats_version
    }

    pub const fn kind(&self) -> StatsCorrelationKind {
        self.kind
    }

    pub const fn strength(&self) -> CorrelationStrengthPermille {
        self.strength
    }

    pub fn columns(&self) -> &[StatsColumnTarget] {
        &self.columns
    }

    pub const fn evidence_bounds(&self) -> CorrelationEvidenceBounds {
        self.evidence_bounds
    }

    pub const fn digest(&self) -> StatsCorrelationDigest {
        self.digest
    }

    // Correlation evidence is advisory only and never authoritative.
    pub const fn is_authoritative(&self) -> bool {
        false
    }

    pub fn is_valid_for(
        &self,
        catalog_version: CatalogVersion,
        stats_version: StatsVersion,
    ) -> bool {
        self.catalog_version == catalog_version && self.stats_version == stats_version
    }

    // Digest byte order/tags are compatibility-critical.
    fn compute_digest(
        id: StatsCorrelationId,
        catalog_version: CatalogVersion,
        stats_version: StatsVersion,
        kind: StatsCorrelationKind,
        strength: CorrelationStrengthPermille,
        columns: &[StatsColumnTarget],
        evidence_bounds: CorrelationEvidenceBounds,
    ) -> StatsCorrelationDigest {
        let mut hasher = Sha256::new();
        hasher.update(STATS_CORRELATION_DOMAIN);
        hasher.update(&[0xE0]);
        hasher.update(&id.get().to_le_bytes());
        hasher.update(&[0xE1]);
        hasher.update(&catalog_version.get().to_le_bytes());
        hasher.update(&[0xE2]);
        hasher.update(&stats_version.get().to_le_bytes());
        hasher.update(&[0xE3]);
        hasher.update(&[kind.as_tag()]);
        hasher.update(&[0xE4]);
        hasher.update(&strength.permille().to_le_bytes());
        hasher.update(&[0xE5]);
        hasher.update(&(columns.len() as u32).to_le_bytes());
        for column in columns {
            hasher.update(&[0xE6]);
            hasher.update(&column.object_id.get().to_le_bytes());
            hasher.update(&column.column_index.to_le_bytes());
        }
        evidence_bounds.absorb(&mut hasher);
        StatsCorrelationDigest::from_bytes(hasher.finalize())
    }
}
