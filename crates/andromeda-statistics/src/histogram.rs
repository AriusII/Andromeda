use andromeda_types::CatalogObjectId;

use andromeda_digest::Sha256;

use super::StatsValidationError;

pub const MAX_HISTOGRAMS_PER_PUBLICATION: usize = 4_096;
pub const MAX_BUCKETS_PER_HISTOGRAM: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SkewMarker {
    Unknown,
    Uniform,
    LowSkew,
    ModerateSkew,
    HighSkew,
    HeavyHitter,
}

impl SkewMarker {
    pub const VARIANT_COUNT: usize = 6;

    pub const fn as_tag(self) -> u8 {
        match self {
            SkewMarker::Unknown => 0x30,
            SkewMarker::Uniform => 0x31,
            SkewMarker::LowSkew => 0x32,
            SkewMarker::ModerateSkew => 0x33,
            SkewMarker::HighSkew => 0x34,
            SkewMarker::HeavyHitter => 0x35,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HistogramBucket {
    pub lower_inclusive: u64,
    pub upper_inclusive: u64,
    pub row_estimate: u64,
    pub distinct_estimate: u64,
}

impl HistogramBucket {
    pub fn validate(self) -> Result<(), StatsValidationError> {
        if self.lower_inclusive > self.upper_inclusive {
            return Err(StatsValidationError::BucketBoundsInverted);
        }
        if self.distinct_estimate > self.row_estimate {
            return Err(StatsValidationError::BucketDistinctExceedsRows);
        }
        Ok(())
    }

    pub fn absorb(self, hasher: &mut Sha256) {
        hasher.update(&[0xB1]);
        hasher.update(&self.lower_inclusive.to_le_bytes());
        hasher.update(&self.upper_inclusive.to_le_bytes());
        hasher.update(&self.row_estimate.to_le_bytes());
        hasher.update(&self.distinct_estimate.to_le_bytes());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistogramPlaceholder {
    buckets: Vec<HistogramBucket>,
    skew: SkewMarker,
}

impl HistogramPlaceholder {
    pub fn new(
        buckets: Vec<HistogramBucket>,
        skew: SkewMarker,
    ) -> Result<Self, StatsValidationError> {
        if buckets.is_empty() {
            return Err(StatsValidationError::HistogramHasNoBuckets);
        }
        if buckets.len() > MAX_BUCKETS_PER_HISTOGRAM {
            return Err(StatsValidationError::HistogramExceedsBucketCap);
        }

        let mut prev_upper: Option<u64> = None;
        for bucket in &buckets {
            bucket.validate()?;
            if let Some(prev) = prev_upper
                && bucket.lower_inclusive <= prev
            {
                return Err(StatsValidationError::BucketsNotMonotonic);
            }
            prev_upper = Some(bucket.upper_inclusive);
        }

        Ok(Self { buckets, skew })
    }

    pub fn buckets(&self) -> &[HistogramBucket] {
        &self.buckets
    }

    pub fn skew(&self) -> SkewMarker {
        self.skew
    }

    pub fn absorb(&self, hasher: &mut Sha256) {
        hasher.update(&[0xB0]);
        hasher.update(&[self.skew.as_tag()]);
        let count = self.buckets.len() as u32;
        hasher.update(&count.to_le_bytes());
        for bucket in &self.buckets {
            bucket.absorb(hasher);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatsColumnTarget {
    pub object_id: CatalogObjectId,
    pub column_index: u16,
}

impl StatsColumnTarget {
    pub const fn new(object_id: CatalogObjectId, column_index: u16) -> Self {
        Self {
            object_id,
            column_index,
        }
    }

    pub const fn canonical_key(self) -> (u64, u16) {
        (self.object_id.get(), self.column_index)
    }
}
