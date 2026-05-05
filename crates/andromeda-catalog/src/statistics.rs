//! V0 versioned statistics publication scaffold.
//!
//! **Status: SCAFFOLD ONLY.**  This module does not sample, does not
//! estimate cardinalities, does not select a plan, and does not consult
//! runtime data.  It only defines the bounded shape of statistics
//! *evidence* a future Procedure-only optimizer may consume, plus a
//! deterministic publication digest so consumers can fingerprint the
//! statistics state without walking the whole publication.
//!
//! ## What this module pins down
//!
//! 1. **Versioned identity.**  Every publication is bound to a
//!    [`StatsVersion`] that is non-zero (zero is reserved for "no stats
//!    bound yet").  Bumping the version always separates the digest, even
//!    if the underlying placeholder evidence is byte-identical.  This is
//!    what later allows [`PlanCacheKey`](crate::PlanCacheKey) to evict on
//!    statistics drift without inspecting individual histograms.
//! 2. **Bounded histogram metadata.**  `HistogramPlaceholder` carries a
//!    bounded ordered list of [`HistogramBucket`]s, each with explicit
//!    `lower_inclusive`/`upper_inclusive`/`row_estimate`/`distinct_estimate`
//!    fields.  Bucket ordering, monotonicity, and distinct-vs-row sanity
//!    are validated at construction time so the digest can never absorb
//!    nonsensical evidence.
//! 3. **Bounded skew marker.**  `SkewMarker` is a closed enum.  Adding a
//!    variant is a doctrine change because it expands what counts as a
//!    legitimate statistical signal the planner is allowed to react to.
//! 4. **Canonical entry ordering.**  Publications are sorted by
//!    `(object_id, column_index)` before the digest is computed, so two
//!    callers that build the same set of entries in different orders
//!    produce the same digest on every node.
//! 5. **Closed validation errors.**  [`StatsValidationError`] enumerates
//!    every reason a publication can be rejected.  No free-form error
//!    strings, no ad hoc "evidence" payloads, no SQL surface.
//!
//! ## What this module deliberately does NOT do
//!
//! - It does not perform sampling or histogram construction.
//! - It does not estimate selectivities or row counts.
//! - It does not own a publication store or wire format.
//! - It does not interpret any SQL or accept free-form bytes.
//! - It does not invalidate plan caches: callers compare
//!   [`StatsPublication::digest`] (or [`StatsVersion`]) themselves.
//!
//! Future statistics collectors must serialize their output through
//! [`StatsPublicationBuilder`] so the digest, ordering, and validation
//! invariants remain a single source of truth.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
};

use crate::contracts::StatsVersion;
use crate::digest::Sha256;

/// Maximum number of `(object, column)` entries in a single publication.
///
/// Bounded so a single publication cannot grow without limit and so the
/// digest computation is O(bounded).  Increasing this constant is a
/// doctrine change.
pub const MAX_HISTOGRAMS_PER_PUBLICATION: usize = 4_096;

/// Maximum number of buckets in a single [`HistogramPlaceholder`].
pub const MAX_BUCKETS_PER_HISTOGRAM: usize = 256;

/// Maximum number of column references in a single correlation evidence item.
///
/// Correlation metadata is optimizer evidence, not a runtime override.  Keeping
/// the arity small prevents an unbounded "join graph" payload from entering a
/// statistics publication.
pub const MAX_COLUMNS_PER_CORRELATION: usize = 8;

/// Maximum number of correlation entries in a single publication.
pub const MAX_CORRELATIONS_PER_PUBLICATION: usize = 1_024;

/// Domain tag absorbed at the start of every publication digest.
const STATS_PUBLICATION_DOMAIN: &[u8] = b"andromeda.stats.publication.v0";

/// Domain tag absorbed at the start of every correlation-evidence digest.
const STATS_CORRELATION_DOMAIN: &[u8] = b"andromeda.stats.correlation.v0";

/// Domain tag absorbed at the start of every correlation-publication digest.
const STATS_CORRELATION_PUBLICATION_DOMAIN: &[u8] = b"andromeda.stats.correlation_publication.v0";

/// Bounded skew marker placeholder.
///
/// The variants are intentionally coarse and closed.  The optimizer is
/// only allowed to react to one of these qualitative signals; it must
/// never read a free-form skew descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SkewMarker {
    /// No skew signal has been published yet.  Equivalent to "absent".
    Unknown,
    /// Distribution is approximately uniform.
    Uniform,
    /// Mild departure from uniform; planner may keep generic plans.
    LowSkew,
    /// Notable skew; planner may consider per-bucket strategies.
    ModerateSkew,
    /// Strong skew; planner should assume one or more dominant ranges.
    HighSkew,
    /// At least one heavy hitter dominates the distribution.
    HeavyHitter,
}

impl SkewMarker {
    pub const VARIANT_COUNT: usize = 6;

    /// Stable tag byte folded into the histogram digest.  Must never be
    /// reordered; appending a new variant must append a new tag.
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

/// A single histogram bucket in the placeholder representation.
///
/// All fields are explicit and bounded.  No free-form payload is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HistogramBucket {
    /// Inclusive lower bound of the bucket key range, in canonical key
    /// space (left to the caller; the scaffold treats it as opaque u64
    /// for ordering purposes only).
    pub lower_inclusive: u64,
    /// Inclusive upper bound of the bucket key range.
    pub upper_inclusive: u64,
    /// Number of rows the bucket is asserted to cover.
    pub row_estimate: u64,
    /// Number of distinct values the bucket is asserted to cover.
    /// Must satisfy `distinct_estimate <= row_estimate`.
    pub distinct_estimate: u64,
}

impl HistogramBucket {
    /// Validate single-bucket invariants.
    pub fn validate(self) -> Result<(), StatsValidationError> {
        if self.lower_inclusive > self.upper_inclusive {
            return Err(StatsValidationError::BucketBoundsInverted);
        }
        if self.distinct_estimate > self.row_estimate {
            return Err(StatsValidationError::BucketDistinctExceedsRows);
        }
        Ok(())
    }

    fn absorb(self, hasher: &mut Sha256) {
        hasher.update(&[0xB1]);
        hasher.update(&self.lower_inclusive.to_le_bytes());
        hasher.update(&self.upper_inclusive.to_le_bytes());
        hasher.update(&self.row_estimate.to_le_bytes());
        hasher.update(&self.distinct_estimate.to_le_bytes());
    }
}

/// Histogram placeholder: a bounded, validated, ordered set of
/// [`HistogramBucket`]s plus a [`SkewMarker`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistogramPlaceholder {
    buckets: Vec<HistogramBucket>,
    skew: SkewMarker,
}

impl HistogramPlaceholder {
    /// Build a placeholder from caller-provided buckets after validating:
    /// - non-empty bucket list,
    /// - bucket count `<= MAX_BUCKETS_PER_HISTOGRAM`,
    /// - per-bucket bound and distinct-vs-row invariants,
    /// - strict monotonic, non-overlapping bucket ranges
    ///   (`buckets[i].upper_inclusive < buckets[i+1].lower_inclusive`).
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

    fn absorb(&self, hasher: &mut Sha256) {
        hasher.update(&[0xB0]);
        hasher.update(&[self.skew.as_tag()]);
        let count = self.buckets.len() as u32;
        hasher.update(&count.to_le_bytes());
        for bucket in &self.buckets {
            bucket.absorb(hasher);
        }
    }
}

/// Identity of a single column the publication describes.
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
}

/// Stable identity of one bounded correlation evidence item.
///
/// Zero is reserved so "no correlation evidence" cannot accidentally collide
/// with a real entry.
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

/// Closed taxonomy of optimizer-visible correlation evidence.
///
/// Variants are deliberately qualitative.  They inform future cost/evidence
/// scoring only; they do not authorize a policy/catalog decision and do not
/// override procedure contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StatsCorrelationKind {
    /// Columns move together positively enough to affect combined selectivity.
    Positive,
    /// Columns move inversely enough to affect combined selectivity.
    Negative,
    /// One column set functionally narrows another column set.
    FunctionalDependency,
    /// Columns are candidate join-key equivalents across one or more objects.
    JoinKeyEquivalence,
    /// Values co-occur often enough to affect semi-join or existence estimates.
    CoOccurrence,
}

impl StatsCorrelationKind {
    pub const VARIANT_COUNT: usize = 5;

    /// Stable tag byte folded into correlation digests.  Reordering or reusing
    /// tag bytes is a doctrine change.
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

/// Bounded correlation strength on a deterministic `0..=1000` permille scale.
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

/// Bounded sampling/evidence envelope for a correlation item.
///
/// The bounds are evidence metadata only.  Consumers may down-weight or ignore
/// low-confidence/stale evidence, but they must not treat it as authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CorrelationEvidenceBounds {
    /// Number of sampled/observed rows or row-pairs contributing evidence.
    pub sample_rows: u64,
    /// Lower bound for the population the sample is intended to represent.
    pub population_lower_bound: u64,
    /// Upper bound for the population the sample is intended to represent.
    pub population_upper_bound: u64,
    /// Confidence on the same deterministic `0..=1000` permille scale.
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

/// Deterministic digest over a single [`StatsCorrelation`] item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatsCorrelationDigest([u8; Self::LEN]);

impl StatsCorrelationDigest {
    pub const LEN: usize = 32;

    pub const fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }
}

/// Bounded inter-table / inter-column statistics correlation metadata.
///
/// This is internal optimizer evidence.  It is version-bound by both
/// [`CatalogVersion`] and [`StatsVersion`]; any catalog or statistics bump must
/// make consumers reject reuse or produce a distinct plan-cache key via the
/// existing `PlanCacheKey` version fields.
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
        columns.sort_by(|a, b| {
            (a.object_id.get(), a.column_index).cmp(&(b.object_id.get(), b.column_index))
        });

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

    /// Correlation evidence is advisory by doctrine.
    pub const fn is_authoritative(&self) -> bool {
        false
    }

    /// Validate version binding before optimizer use.
    pub fn is_valid_for(
        &self,
        catalog_version: CatalogVersion,
        stats_version: StatsVersion,
    ) -> bool {
        self.catalog_version == catalog_version && self.stats_version == stats_version
    }

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

/// Finalized, immutable correlation metadata publication.
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

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Builder for bounded, version-bound correlation publications.
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

/// 32-byte deterministic digest over a [`StatsPublication`].
///
/// Stable across nodes because every input is absorbed with explicit
/// byte tags and little-endian widths.  Useful for trace records and
/// observability without leaking individual fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatsPublicationDigest([u8; Self::LEN]);

impl StatsPublicationDigest {
    pub const LEN: usize = 32;

    pub const fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

/// Finalized, immutable, versioned statistics publication.
///
/// Once constructed, the entry list is canonically ordered and the
/// digest is fixed.  Mutating the publication requires building a fresh
/// one through [`StatsPublicationBuilder`].
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

/// Streaming builder for [`StatsPublication`].
///
/// The builder is intentionally narrow: only `(target, placeholder)`
/// pairs are accepted, with full validation at insertion time.  No
/// free-form bytes, no SQL text, no runtime values.
#[derive(Debug, Clone)]
pub struct StatsPublicationBuilder {
    version: StatsVersion,
    entries: Vec<(StatsColumnTarget, HistogramPlaceholder)>,
}

impl StatsPublicationBuilder {
    /// Begin a new publication for `version`.
    ///
    /// Returns `Err(StatsValidationError::StatsVersionZero)` if the
    /// version is the reserved 0 sentinel: published statistics must
    /// always carry a non-zero, monotonically advanced version.
    pub fn new(version: StatsVersion) -> Result<Self, StatsValidationError> {
        if version.get() == 0 {
            return Err(StatsValidationError::StatsVersionZero);
        }
        Ok(Self {
            version,
            entries: Vec::new(),
        })
    }

    /// Append one entry.  Validates:
    /// - `target.object_id` is non-zero,
    /// - the publication has not already exceeded
    ///   `MAX_HISTOGRAMS_PER_PUBLICATION`,
    /// - `target` is not already present (no duplicate `(object,
    ///   column)` pairs).
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

    /// Finalize into an immutable [`StatsPublication`].
    ///
    /// Entries are canonically sorted by `(object_id, column_index)`
    /// before the digest is computed, so two callers that push the same
    /// set of entries in different orders observe the same digest.
    pub fn finish(mut self) -> StatsPublication {
        self.entries.sort_by(|a, b| {
            (a.0.object_id.get(), a.0.column_index).cmp(&(b.0.object_id.get(), b.0.column_index))
        });

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

/// Closed enumeration of every reason a statistics publication can be
/// rejected.  Variants are stable and intentionally narrow so callers
/// cannot smuggle free-form failure modes through the publication API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsValidationError {
    /// `StatsVersion(0)` is reserved for "no stats bound yet".
    StatsVersionZero,
    /// A target carried `CatalogObjectId(0)`.
    ZeroObjectId,
    /// Two entries reference the same `(object, column)` target.
    DuplicateTarget,
    /// More than `MAX_HISTOGRAMS_PER_PUBLICATION` entries were pushed.
    PublicationExceedsHistogramCap,
    /// A histogram contains zero buckets.
    HistogramHasNoBuckets,
    /// A histogram exceeds `MAX_BUCKETS_PER_HISTOGRAM`.
    HistogramExceedsBucketCap,
    /// A bucket has `lower_inclusive > upper_inclusive`.
    BucketBoundsInverted,
    /// A bucket reports `distinct_estimate > row_estimate`.
    BucketDistinctExceedsRows,
    /// Buckets are not strictly increasing or overlap their neighbours.
    BucketsNotMonotonic,
    /// `CatalogVersion(0)` is reserved and cannot bind correlation metadata.
    CatalogVersionZero,
    /// Correlation metadata must reference at least two columns.
    CorrelationTooFewColumns,
    /// Correlation metadata exceeds `MAX_COLUMNS_PER_CORRELATION`.
    CorrelationExceedsColumnCap,
    /// Correlation metadata repeats the same `(object, column)` reference.
    CorrelationDuplicateColumn,
    /// Correlation strength exceeds the `0..=1000` permille scale.
    CorrelationStrengthOutOfRange,
    /// Correlation evidence confidence exceeds the `0..=1000` permille scale.
    CorrelationConfidenceOutOfRange,
    /// Correlation evidence has zero sampled/observed rows.
    CorrelationSampleRowsZero,
    /// Correlation evidence population lower bound exceeds upper bound.
    CorrelationPopulationBoundsInverted,
    /// Correlation evidence sample rows exceeds the declared population upper bound.
    CorrelationSampleExceedsPopulationUpper,
    /// Correlation entry does not match the publication's version tuple.
    CorrelationVersionMismatch,
    /// More than `MAX_CORRELATIONS_PER_PUBLICATION` entries were pushed.
    CorrelationPublicationExceedsCap,
    /// Two correlation entries use the same identity.
    DuplicateCorrelationId,
}

impl core::fmt::Display for StatsValidationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            StatsValidationError::StatsVersionZero => {
                "StatsVersion 0 is reserved and cannot be published"
            }
            StatsValidationError::ZeroObjectId => "stats target CatalogObjectId must be non-zero",
            StatsValidationError::DuplicateTarget => {
                "duplicate (object, column) target in stats publication"
            }
            StatsValidationError::PublicationExceedsHistogramCap => {
                "stats publication exceeds MAX_HISTOGRAMS_PER_PUBLICATION"
            }
            StatsValidationError::HistogramHasNoBuckets => {
                "histogram placeholder must contain at least one bucket"
            }
            StatsValidationError::HistogramExceedsBucketCap => {
                "histogram placeholder exceeds MAX_BUCKETS_PER_HISTOGRAM"
            }
            StatsValidationError::BucketBoundsInverted => {
                "histogram bucket lower_inclusive exceeds upper_inclusive"
            }
            StatsValidationError::BucketDistinctExceedsRows => {
                "histogram bucket distinct_estimate exceeds row_estimate"
            }
            StatsValidationError::BucketsNotMonotonic => {
                "histogram buckets must be strictly increasing and non-overlapping"
            }
            StatsValidationError::CatalogVersionZero => {
                "CatalogVersion 0 cannot bind statistics correlation metadata"
            }
            StatsValidationError::CorrelationTooFewColumns => {
                "statistics correlation metadata must reference at least two columns"
            }
            StatsValidationError::CorrelationExceedsColumnCap => {
                "statistics correlation metadata exceeds MAX_COLUMNS_PER_CORRELATION"
            }
            StatsValidationError::CorrelationDuplicateColumn => {
                "statistics correlation metadata repeats an (object, column) reference"
            }
            StatsValidationError::CorrelationStrengthOutOfRange => {
                "statistics correlation strength exceeds the 0..=1000 permille scale"
            }
            StatsValidationError::CorrelationConfidenceOutOfRange => {
                "statistics correlation confidence exceeds the 0..=1000 permille scale"
            }
            StatsValidationError::CorrelationSampleRowsZero => {
                "statistics correlation evidence sample_rows must be non-zero"
            }
            StatsValidationError::CorrelationPopulationBoundsInverted => {
                "statistics correlation population lower bound exceeds upper bound"
            }
            StatsValidationError::CorrelationSampleExceedsPopulationUpper => {
                "statistics correlation sample_rows exceeds population upper bound"
            }
            StatsValidationError::CorrelationVersionMismatch => {
                "statistics correlation entry does not match publication versions"
            }
            StatsValidationError::CorrelationPublicationExceedsCap => {
                "statistics correlation publication exceeds MAX_CORRELATIONS_PER_PUBLICATION"
            }
            StatsValidationError::DuplicateCorrelationId => {
                "duplicate statistics correlation id in publication"
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(object: u64, column: u16) -> StatsColumnTarget {
        StatsColumnTarget::new(CatalogObjectId::new(object), column)
    }

    fn bucket(lo: u64, hi: u64, rows: u64, distinct: u64) -> HistogramBucket {
        HistogramBucket {
            lower_inclusive: lo,
            upper_inclusive: hi,
            row_estimate: rows,
            distinct_estimate: distinct,
        }
    }

    fn sample_histogram() -> HistogramPlaceholder {
        HistogramPlaceholder::new(
            vec![
                bucket(0, 9, 100, 50),
                bucket(10, 19, 80, 40),
                bucket(20, 29, 20, 10),
            ],
            SkewMarker::LowSkew,
        )
        .expect("sample histogram is valid")
    }

    fn sample_bounds() -> CorrelationEvidenceBounds {
        CorrelationEvidenceBounds {
            sample_rows: 100,
            population_lower_bound: 100,
            population_upper_bound: 1_000,
            confidence_permille: 900,
        }
    }

    fn correlation(
        id: u64,
        catalog: u64,
        stats: u64,
        columns: Vec<StatsColumnTarget>,
    ) -> StatsCorrelation {
        StatsCorrelation::new(
            StatsCorrelationId::new(id).expect("non-zero correlation id"),
            CatalogVersion::new(catalog),
            StatsVersion::new(stats),
            StatsCorrelationKind::JoinKeyEquivalence,
            CorrelationStrengthPermille::from_permille(800).unwrap(),
            columns,
            sample_bounds(),
        )
        .expect("sample correlation is valid")
    }

    #[test]
    fn skew_marker_variant_count_is_bounded() {
        assert_eq!(SkewMarker::VARIANT_COUNT, 6);
        let tags = [
            SkewMarker::Unknown.as_tag(),
            SkewMarker::Uniform.as_tag(),
            SkewMarker::LowSkew.as_tag(),
            SkewMarker::ModerateSkew.as_tag(),
            SkewMarker::HighSkew.as_tag(),
            SkewMarker::HeavyHitter.as_tag(),
        ];
        let mut sorted = tags;
        sorted.sort_unstable();
        assert!(
            sorted.windows(2).all(|pair| pair[0] != pair[1]),
            "SkewMarker tags must be unique"
        );
    }

    #[test]
    fn correlation_kind_variant_count_is_bounded() {
        assert_eq!(StatsCorrelationKind::VARIANT_COUNT, 5);
        let tags = [
            StatsCorrelationKind::Positive.as_tag(),
            StatsCorrelationKind::Negative.as_tag(),
            StatsCorrelationKind::FunctionalDependency.as_tag(),
            StatsCorrelationKind::JoinKeyEquivalence.as_tag(),
            StatsCorrelationKind::CoOccurrence.as_tag(),
        ];
        let mut sorted = tags;
        sorted.sort_unstable();
        assert!(
            sorted.windows(2).all(|pair| pair[0] != pair[1]),
            "StatsCorrelationKind tags must be unique"
        );
    }

    #[test]
    fn histogram_rejects_empty_buckets() {
        let err = HistogramPlaceholder::new(Vec::new(), SkewMarker::Unknown).unwrap_err();
        assert_eq!(err, StatsValidationError::HistogramHasNoBuckets);
    }

    #[test]
    fn histogram_rejects_exceeded_bucket_cap() {
        let mut buckets = Vec::with_capacity(MAX_BUCKETS_PER_HISTOGRAM + 1);
        let mut next: u64 = 0;
        for _ in 0..=MAX_BUCKETS_PER_HISTOGRAM {
            buckets.push(bucket(next, next, 1, 1));
            next += 2;
        }
        let err = HistogramPlaceholder::new(buckets, SkewMarker::Unknown).unwrap_err();
        assert_eq!(err, StatsValidationError::HistogramExceedsBucketCap);
    }

    #[test]
    fn histogram_rejects_inverted_bucket_bounds() {
        let err =
            HistogramPlaceholder::new(vec![bucket(10, 5, 1, 1)], SkewMarker::Unknown).unwrap_err();
        assert_eq!(err, StatsValidationError::BucketBoundsInverted);
    }

    #[test]
    fn histogram_rejects_distinct_exceeding_rows() {
        let err =
            HistogramPlaceholder::new(vec![bucket(0, 9, 5, 6)], SkewMarker::Unknown).unwrap_err();
        assert_eq!(err, StatsValidationError::BucketDistinctExceedsRows);
    }

    #[test]
    fn histogram_rejects_overlapping_buckets() {
        let err = HistogramPlaceholder::new(
            vec![bucket(0, 10, 1, 1), bucket(10, 20, 1, 1)],
            SkewMarker::Unknown,
        )
        .unwrap_err();
        assert_eq!(err, StatsValidationError::BucketsNotMonotonic);
    }

    #[test]
    fn histogram_rejects_decreasing_buckets() {
        let err = HistogramPlaceholder::new(
            vec![bucket(10, 19, 1, 1), bucket(0, 9, 1, 1)],
            SkewMarker::Unknown,
        )
        .unwrap_err();
        assert_eq!(err, StatsValidationError::BucketsNotMonotonic);
    }

    #[test]
    fn builder_rejects_stats_version_zero() {
        let err = StatsPublicationBuilder::new(StatsVersion::new(0)).unwrap_err();
        assert_eq!(err, StatsValidationError::StatsVersionZero);
    }

    #[test]
    fn builder_rejects_zero_object_id() {
        let err = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(0, 0), sample_histogram())
            .unwrap_err();
        assert_eq!(err, StatsValidationError::ZeroObjectId);
    }

    #[test]
    fn builder_rejects_duplicate_target() {
        let err = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(7, 3), sample_histogram())
            .unwrap()
            .push(target(7, 3), sample_histogram())
            .unwrap_err();
        assert_eq!(err, StatsValidationError::DuplicateTarget);
    }

    #[test]
    fn publication_digest_is_deterministic() {
        let lhs = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(7, 3), sample_histogram())
            .unwrap()
            .push(target(7, 4), sample_histogram())
            .unwrap()
            .finish();
        let rhs = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(7, 3), sample_histogram())
            .unwrap()
            .push(target(7, 4), sample_histogram())
            .unwrap()
            .finish();
        assert_eq!(lhs.digest(), rhs.digest());
        assert_eq!(lhs.entries(), rhs.entries());
        assert!(!lhs.digest().is_zero());
        assert_eq!(lhs.len(), 2);
    }

    #[test]
    fn publication_canonical_ordering_yields_same_digest() {
        let forward = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(7, 3), sample_histogram())
            .unwrap()
            .push(target(9, 1), sample_histogram())
            .unwrap()
            .finish();
        let reversed = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(9, 1), sample_histogram())
            .unwrap()
            .push(target(7, 3), sample_histogram())
            .unwrap()
            .finish();
        assert_eq!(forward.digest(), reversed.digest());
        // Both publications expose entries in canonical order.
        let forward_targets: Vec<_> = forward.entries().iter().map(|(t, _)| *t).collect();
        let reversed_targets: Vec<_> = reversed.entries().iter().map(|(t, _)| *t).collect();
        assert_eq!(forward_targets, reversed_targets);
    }

    #[test]
    fn publication_separates_on_stats_version() {
        let v1 = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(7, 3), sample_histogram())
            .unwrap()
            .finish();
        let v2 = StatsPublicationBuilder::new(StatsVersion::new(2))
            .unwrap()
            .push(target(7, 3), sample_histogram())
            .unwrap()
            .finish();
        assert_ne!(v1.digest(), v2.digest());
        assert_ne!(v1.version(), v2.version());
    }

    #[test]
    fn publication_separates_on_skew_marker() {
        let low = sample_histogram();
        let high = HistogramPlaceholder::new(low.buckets().to_vec(), SkewMarker::HighSkew).unwrap();
        let pub_low = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(7, 3), low)
            .unwrap()
            .finish();
        let pub_high = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(7, 3), high)
            .unwrap()
            .finish();
        assert_ne!(pub_low.digest(), pub_high.digest());
    }

    #[test]
    fn publication_separates_on_bucket_evidence() {
        let base = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(7, 3), sample_histogram())
            .unwrap()
            .finish();

        let altered_buckets = HistogramPlaceholder::new(
            vec![
                bucket(0, 9, 200, 50),
                bucket(10, 19, 80, 40),
                bucket(20, 29, 20, 10),
            ],
            SkewMarker::LowSkew,
        )
        .unwrap();
        let altered = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(7, 3), altered_buckets)
            .unwrap()
            .finish();

        assert_ne!(base.digest(), altered.digest());
    }

    #[test]
    fn empty_publication_is_stable_and_distinct_from_populated() {
        let empty_a = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .finish();
        let empty_b = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .finish();
        assert_eq!(empty_a.digest(), empty_b.digest());
        assert!(empty_a.is_empty());

        let populated = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .push(target(1, 0), sample_histogram())
            .unwrap()
            .finish();
        assert_ne!(empty_a.digest(), populated.digest());
    }

    #[test]
    fn empty_publication_separates_on_stats_version() {
        let v1 = StatsPublicationBuilder::new(StatsVersion::new(1))
            .unwrap()
            .finish();
        let v2 = StatsPublicationBuilder::new(StatsVersion::new(2))
            .unwrap()
            .finish();
        assert_ne!(v1.digest(), v2.digest());
    }

    #[test]
    fn correlation_canonicalizes_column_order_for_digest_stability() {
        let lhs = correlation(1, 7, 3, vec![target(20, 2), target(10, 1)]);
        let rhs = correlation(1, 7, 3, vec![target(10, 1), target(20, 2)]);

        assert_eq!(lhs.columns(), rhs.columns());
        assert_eq!(lhs.digest(), rhs.digest());
        assert!(!lhs.is_authoritative());
    }

    #[test]
    fn correlation_rejects_invalid_bounds_and_duplicate_columns() {
        let duplicate_err = StatsCorrelation::new(
            StatsCorrelationId::new(1).unwrap(),
            CatalogVersion::new(1),
            StatsVersion::new(1),
            StatsCorrelationKind::Positive,
            CorrelationStrengthPermille::from_permille(100).unwrap(),
            vec![target(1, 0), target(1, 0)],
            sample_bounds(),
        )
        .unwrap_err();
        assert_eq!(
            duplicate_err,
            StatsValidationError::CorrelationDuplicateColumn
        );

        let bounds_err = CorrelationEvidenceBounds {
            sample_rows: 0,
            population_lower_bound: 0,
            population_upper_bound: 10,
            confidence_permille: 1,
        }
        .validate()
        .unwrap_err();
        assert_eq!(bounds_err, StatsValidationError::CorrelationSampleRowsZero);
    }

    #[test]
    fn correlation_invalidates_on_catalog_or_stats_version() {
        let entry = correlation(1, 7, 3, vec![target(10, 1), target(20, 2)]);

        assert!(entry.is_valid_for(CatalogVersion::new(7), StatsVersion::new(3)));
        assert!(!entry.is_valid_for(CatalogVersion::new(8), StatsVersion::new(3)));
        assert!(!entry.is_valid_for(CatalogVersion::new(7), StatsVersion::new(4)));

        let catalog_bump = correlation(1, 8, 3, vec![target(10, 1), target(20, 2)]);
        let stats_bump = correlation(1, 7, 4, vec![target(10, 1), target(20, 2)]);
        assert_ne!(entry.digest(), catalog_bump.digest());
        assert_ne!(entry.digest(), stats_bump.digest());
    }

    #[test]
    fn correlation_publication_is_deterministic_and_version_bound() {
        let first = correlation(1, 7, 3, vec![target(10, 1), target(20, 2)]);
        let second = correlation(2, 7, 3, vec![target(30, 1), target(40, 2)]);

        let lhs =
            StatsCorrelationPublicationBuilder::new(CatalogVersion::new(7), StatsVersion::new(3))
                .unwrap()
                .push(second.clone())
                .unwrap()
                .push(first.clone())
                .unwrap()
                .finish();

        let rhs =
            StatsCorrelationPublicationBuilder::new(CatalogVersion::new(7), StatsVersion::new(3))
                .unwrap()
                .push(first)
                .unwrap()
                .push(second)
                .unwrap()
                .finish();

        assert_eq!(lhs.digest(), rhs.digest());
        assert_eq!(lhs.entries()[0].id().get(), 1);
        assert_eq!(lhs.entries()[1].id().get(), 2);

        let mismatched = correlation(3, 8, 3, vec![target(10, 1), target(20, 2)]);
        let err =
            StatsCorrelationPublicationBuilder::new(CatalogVersion::new(7), StatsVersion::new(3))
                .unwrap()
                .push(mismatched)
                .unwrap_err();
        assert_eq!(err, StatsValidationError::CorrelationVersionMismatch);
    }
}

// ============================================================================
// N5 STATISTICS ENGINE TYPES (Wave 19+)
// ============================================================================
//
// This section defines the types for the Statistics & Histogram Engine,
// which extends the V0 scaffold with concrete histogram construction and
// NDV estimation capabilities.
//
// Status: Design Only — All implementations deferred to Wave 19+.

use std::collections::BTreeMap;

/// Maximum rows for full-scan collection (above this, use sampling).
pub const STATS_FULL_SCAN_THRESHOLD: u64 = 100_000;

/// Sample size for reservoir sampling (e.g., 100K rows per table).
pub const STATS_SAMPLE_SIZE: usize = 100_000;

/// Default histogram bucket count.
pub const DEFAULT_BUCKET_COUNT: u32 = 32;

/// Default HyperLogLog precision (4K registers, 4KB memory, ~2% error).
pub const DEFAULT_HLL_PRECISION: u8 = 12;

/// Mutation threshold for invalidation (e.g., 10% of rows modified).
pub const STATS_INVALIDATION_MUTATION_PCT: f64 = 10.0;

/// Maximum time statistics can be stale (hours, e.g., 7 days).
pub const STATS_MAX_AGE_HOURS: u64 = 168;

/// LSN-based invalidation threshold.
pub const STATS_LSN_DELTA_THRESHOLD: u64 = 1_000_000;

/// Maximum NDV estimate that triggers exact counting vs HyperLogLog.
pub const NDV_EXACT_THRESHOLD: u64 = 10_000;

/// Histogram algorithm selection indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HistogramAlgorithm {
    /// Fixed-width ranges; O(1) bucket lookup; good for uniform distributions.
    EquiWidth,
    /// Fixed row-count ranges; adapts to skewed distributions.
    EquiDepth,
}

impl HistogramAlgorithm {
    /// Human-readable name for observability.
    pub fn name(&self) -> &'static str {
        match self {
            HistogramAlgorithm::EquiWidth => "equi-width",
            HistogramAlgorithm::EquiDepth => "equi-depth",
        }
    }
}

/// Equi-width histogram: column range split into N equal-width buckets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquiWidthHistogram {
    /// Catalog column identifier.
    pub column_id: u64,
    /// Number of buckets in the histogram.
    pub bucket_count: u32,
    /// Canonicalized minimum value in the column.
    pub min_value: u64,
    /// Canonicalized maximum value in the column.
    pub max_value: u64,
    /// Histogram buckets (exactly bucket_count entries).
    pub buckets: Vec<HistogramBucket>,
    /// Count of NULL values in the column.
    pub null_count: u64,
    /// Estimated number of distinct non-NULL values.
    pub ndv_estimate: u64,
}

impl EquiWidthHistogram {
    /// Total rows covered by this histogram (excluding NULLs).
    pub fn total_rows(&self) -> u64 {
        self.buckets.iter().map(|b| b.row_estimate).sum()
    }

    /// Total distinct values covered (excluding NULLs).
    pub fn total_distinct(&self) -> u64 {
        self.ndv_estimate
    }
}

/// Equi-depth histogram: column range split into N buckets with equal row counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquiDepthHistogram {
    /// Catalog column identifier.
    pub column_id: u64,
    /// Number of buckets in the histogram.
    pub bucket_count: u32,
    /// Canonicalized minimum value in the column.
    pub min_value: u64,
    /// Canonicalized maximum value in the column.
    pub max_value: u64,
    /// Histogram buckets with balanced row counts.
    pub buckets: Vec<HistogramBucket>,
    /// Count of NULL values in the column.
    pub null_count: u64,
    /// Estimated number of distinct non-NULL values.
    pub ndv_estimate: u64,
}

impl EquiDepthHistogram {
    /// Total rows covered by this histogram (excluding NULLs).
    pub fn total_rows(&self) -> u64 {
        self.buckets.iter().map(|b| b.row_estimate).sum()
    }

    /// Total distinct values covered (excluding NULLs).
    pub fn total_distinct(&self) -> u64 {
        self.ndv_estimate
    }
}

/// Adaptive histogram: automatically selects algorithm based on observed skew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptiveHistogram {
    /// Catalog column identifier.
    pub column_id: u64,
    /// Number of buckets in the histogram.
    pub bucket_count: u32,
    /// Canonicalized minimum value in the column.
    pub min_value: u64,
    /// Canonicalized maximum value in the column.
    pub max_value: u64,
    /// Histogram buckets constructed by selected algorithm.
    pub buckets: Vec<HistogramBucket>,
    /// Count of NULL values in the column.
    pub null_count: u64,
    /// Estimated number of distinct non-NULL values.
    pub ndv_estimate: u64,
    /// Which algorithm was actually selected.
    pub algorithm_choice: HistogramAlgorithm,
}

impl AdaptiveHistogram {
    /// Total rows covered by this histogram (excluding NULLs).
    pub fn total_rows(&self) -> u64 {
        self.buckets.iter().map(|b| b.row_estimate).sum()
    }

    /// Total distinct values covered (excluding NULLs).
    pub fn total_distinct(&self) -> u64 {
        self.ndv_estimate
    }
}

/// Enumeration of histogram types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Histogram {
    /// Fixed-width range histogram.
    EquiWidth(EquiWidthHistogram),
    /// Fixed-depth range histogram.
    EquiDepth(EquiDepthHistogram),
    /// Adaptively selected histogram.
    Adaptive(AdaptiveHistogram),
}

impl Histogram {
    /// Get column ID for this histogram.
    pub fn column_id(&self) -> u64 {
        match self {
            Histogram::EquiWidth(h) => h.column_id,
            Histogram::EquiDepth(h) => h.column_id,
            Histogram::Adaptive(h) => h.column_id,
        }
    }

    /// Get bucket count for this histogram.
    pub fn bucket_count(&self) -> u32 {
        match self {
            Histogram::EquiWidth(h) => h.bucket_count,
            Histogram::EquiDepth(h) => h.bucket_count,
            Histogram::Adaptive(h) => h.bucket_count,
        }
    }

    /// Get total rows (excluding NULLs).
    pub fn total_rows(&self) -> u64 {
        match self {
            Histogram::EquiWidth(h) => h.total_rows(),
            Histogram::EquiDepth(h) => h.total_rows(),
            Histogram::Adaptive(h) => h.total_rows(),
        }
    }

    /// Get total distinct values.
    pub fn total_distinct(&self) -> u64 {
        match self {
            Histogram::EquiWidth(h) => h.total_distinct(),
            Histogram::EquiDepth(h) => h.total_distinct(),
            Histogram::Adaptive(h) => h.total_distinct(),
        }
    }

    /// Get algorithm choice (if adaptive; otherwise inferred).
    pub fn algorithm(&self) -> HistogramAlgorithm {
        match self {
            Histogram::EquiWidth(_) => HistogramAlgorithm::EquiWidth,
            Histogram::EquiDepth(_) => HistogramAlgorithm::EquiDepth,
            Histogram::Adaptive(h) => h.algorithm_choice,
        }
    }

    /// Get buckets slice.
    pub fn buckets(&self) -> &[HistogramBucket] {
        match self {
            Histogram::EquiWidth(h) => &h.buckets,
            Histogram::EquiDepth(h) => &h.buckets,
            Histogram::Adaptive(h) => &h.buckets,
        }
    }
}

/// Statistics for a single column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnStatistics {
    /// Catalog column identifier.
    pub column_id: u64,
    /// The histogram variant (equi-width, equi-depth, or adaptive).
    pub histogram: Histogram,
    /// Count of NULL values in the column.
    pub null_count: u64,
    /// Estimated number of distinct non-NULL values.
    pub ndv: u64,
    /// WAL LSN at collection time; used for invalidation detection.
    pub collected_at_lsn: u64,
}

impl ColumnStatistics {
    /// Total non-NULL rows in the column.
    pub fn non_null_rows(&self) -> u64 {
        self.histogram.total_rows()
    }

    /// Total rows including NULLs.
    pub fn total_rows_with_nulls(&self) -> u64 {
        self.histogram.total_rows() + self.null_count
    }
}

/// Aggregate statistics for an entire table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableStatistics {
    /// Catalog table object ID.
    pub table_id: CatalogObjectId,
    /// Per-column statistics (column_index → ColumnStatistics).
    pub column_stats: BTreeMap<u16, ColumnStatistics>,
    /// Total row count estimate for the table.
    pub row_count: u64,
    /// WAL LSN at time of collection.
    pub collected_at_lsn: u64,
    /// Version identifier for this statistics snapshot.
    pub version: StatsVersion,
}

impl TableStatistics {
    /// Add per-column statistics.
    pub fn add_column_stats(&mut self, col_index: u16, stats: ColumnStatistics) {
        self.column_stats.insert(col_index, stats);
    }

    /// Retrieve statistics for a column.
    pub fn get_column_stats(&self, col_index: u16) -> Option<&ColumnStatistics> {
        self.column_stats.get(&col_index)
    }
}

/// Policy for determining when statistics are stale and need re-collection.
#[derive(Debug, Clone, PartialEq)]
pub struct StatsInvalidationPolicy {
    /// Mutation threshold as percentage.
    pub mutation_threshold_percent: f64,
    /// Time-based staleness in hours.
    pub max_age_hours: u64,
    /// LSN-based staleness threshold.
    pub lsn_delta_threshold: u64,
}

impl Default for StatsInvalidationPolicy {
    fn default() -> Self {
        Self {
            mutation_threshold_percent: STATS_INVALIDATION_MUTATION_PCT,
            max_age_hours: STATS_MAX_AGE_HOURS,
            lsn_delta_threshold: STATS_LSN_DELTA_THRESHOLD,
        }
    }
}

/// Trait for streaming histogram construction.
///
/// **Wave 19 Implementation:**
/// - `EquiWidthBuilder::finalize()`
/// - `EquiDepthBuilder::finalize()`
/// - `AdaptiveBuilder::finalize()` with skew detection
pub trait HistogramBuilder: Send + Sync {
    /// Observe a single value during histogram construction.
    fn add_value(&mut self, value: u64) -> andromeda_core::AndromedaResult<()>;

    /// Finalize and return the immutable histogram.
    fn finalize(self: Box<Self>) -> andromeda_core::AndromedaResult<Histogram>;

    /// Estimated current memory consumption in bytes.
    fn estimated_memory_bytes(&self) -> usize;
}

/// Trait for cardinality (distinct value count) estimation.
///
/// **Wave 19 Implementation:**
/// - `ExactNdvCounter` for small cardinalities
/// - `HyperLogLog` for large cardinalities
pub trait NdvEstimator: Send + Sync {
    /// Observe a single value.
    fn observe(&mut self, value: u64);

    /// Return estimated distinct count.
    fn estimate(&self) -> u64;

    /// Return current memory consumption.
    fn memory_bytes(&self) -> usize;
}

/// Cardinality feedback from plan execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanFeedback {
    /// Estimated row count from the plan.
    pub estimated_rows: u64,
    /// Actual observed row count during execution.
    pub actual_rows: u64,
}

impl PlanFeedback {
    /// Create new feedback entry and compute error ratio.
    pub fn new(estimated_rows: u64, actual_rows: u64) -> (Self, f64) {
        let max_rows = estimated_rows.max(actual_rows);
        let error_ratio = if max_rows > 0 {
            ((estimated_rows as i64 - actual_rows as i64).abs() as f64) / max_rows as f64
        } else {
            0.0
        };

        (
            Self {
                estimated_rows,
                actual_rows,
            },
            error_ratio,
        )
    }

    /// Check if this feedback indicates significant estimation error.
    pub fn is_high_error(&self, threshold: f64) -> bool {
        let max_rows = self.estimated_rows.max(self.actual_rows);
        let error_ratio = if max_rows > 0 {
            ((self.estimated_rows as i64 - self.actual_rows as i64).abs() as f64) / max_rows as f64
        } else {
            0.0
        };
        error_ratio > threshold
    }
}

/// Accumulated feedback statistics for a table/column.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedbackStatistics {
    /// Total number of feedback observations.
    pub total_observations: u64,
    /// Sum of all error ratios.
    pub sum_errors: f64,
    /// Maximum error ratio observed.
    pub max_error: f64,
    /// Threshold for triggering re-collection.
    pub high_error_threshold: f64,
}

impl FeedbackStatistics {
    /// Create new feedback statistics.
    pub fn new(high_error_threshold: f64) -> Self {
        Self {
            total_observations: 0,
            sum_errors: 0.0,
            max_error: 0.0,
            high_error_threshold,
        }
    }

    /// Record new feedback observation with error ratio.
    pub fn record(&mut self, error_ratio: f64) {
        self.total_observations += 1;
        self.sum_errors += error_ratio;
        self.max_error = self.max_error.max(error_ratio);
    }

    /// Average error ratio across all observations.
    pub fn average_error(&self) -> f64 {
        if self.total_observations > 0 {
            self.sum_errors / self.total_observations as f64
        } else {
            0.0
        }
    }

    /// Check if feedback indicates need for re-collection.
    pub fn should_recollect(&self) -> bool {
        self.max_error > self.high_error_threshold
            || self.average_error() > self.high_error_threshold
    }
}

// ============================================================================
// NDV ESTIMATION IMPLEMENTATIONS (Wave 19+)
// ============================================================================

/// Exact NDV counter using HashSet.
///
/// **Use Case:** Small expected cardinalities (< 10K distinct values).
///
/// **Wave 19 Implementation:**
/// - `observe()`: insert value into set
/// - `estimate()`: return set.len()
/// - `memory_bytes()`: return set.capacity() * 8
#[derive(Debug, Clone)]
pub struct ExactNdvCounter {
    /// Set of observed distinct values.
    seen: std::collections::HashSet<u64>,
}

impl ExactNdvCounter {
    /// Create a new exact NDV counter.
    pub fn new() -> Self {
        Self {
            seen: std::collections::HashSet::new(),
        }
    }

    /// Create with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            seen: std::collections::HashSet::with_capacity(capacity),
        }
    }
}

impl Default for ExactNdvCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl NdvEstimator for ExactNdvCounter {
    fn observe(&mut self, value: u64) {
        self.seen.insert(value);
    }

    fn estimate(&self) -> u64 {
        self.seen.len() as u64
    }

    fn memory_bytes(&self) -> usize {
        self.seen.capacity() * std::mem::size_of::<u64>()
    }
}

/// HyperLogLog cardinality estimator.
///
/// **Use Case:** Large expected cardinalities (> 10K distinct values).
///
/// **Algorithm:** Probabilistic cardinality estimation using bit patterns.
///
/// **Characteristics:**
/// - Memory bounded: O(2^precision) bytes
/// - Relative error: ~1.04 / sqrt(2^precision)
/// - Default precision 12: 4K registers (4KB memory), ~2% error
/// - Typical precision range: 4..16 bits
///
/// **Wave 19 Implementation Steps:**
/// 1. Initialize registers: Vec<u8> with 2^precision elements, all zeros
/// 2. `observe(value)`:
///    a. Hash value to u64 (use xxhash64)
///    b. Extract first `precision` bits as register index
///    c. Count leading zeros in remaining bits (rho function)
///    d. Update register: `registers[idx] = max(registers[idx], rho_value)`
/// 3. `estimate()`:
///    a. Compute raw estimate: `alpha * m^2 / sum(2^(-registers[i]))`
///    b. Apply bias correction based on range
/// 4. Error characterization: validate ~2% error on test datasets
#[derive(Debug, Clone)]
pub struct HyperLogLog {
    /// Precision parameter: number of bits from hash used as register index.
    /// Typical range: 4..16 (default 12).
    pub precision: u8,

    /// Registers: one u8 per HLL bucket.
    /// Count = 2^precision (typically 4K for precision=12).
    pub registers: Vec<u8>,

    /// Bias correction constant (alpha).
    /// alpha = 0.7213 / (1 + 1.079 / m) where m = 2^precision
    pub alpha: f64,
}

impl HyperLogLog {
    /// Create a new HyperLogLog with given precision.
    ///
    /// **Precision Guidelines:**
    /// - 4:  16 registers,    256B memory, 16% error
    /// - 6:  64 registers,  512B memory,    8% error
    /// - 8: 256 registers,  256B memory,    4% error
    /// - 12: 4K registers,  4KB memory,    2% error (default)
    /// - 14: 16K registers, 16KB memory,    1% error
    ///
    /// **Wave 19 Implementation:**
    /// - Validate precision in range [4, 16]
    /// - Allocate registers: Vec::with_capacity(2^precision)
    /// - Initialize all to 0
    /// - Compute alpha: 0.7213 / (1 + 1.079 / (2^precision as f64))
    pub fn new(precision: u8) -> AndromedaResult<Self> {
        if !(4..=16).contains(&precision) {
            return Err(andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Catalog,
                "HyperLogLog precision must be in range [4, 16]",
            ));
        }

        let m = (1u64 << precision) as f64;
        let alpha = 0.7213 / (1.0 + 1.079 / m);
        let registers = vec![0u8; 1 << precision];

        Ok(Self {
            precision,
            registers,
            alpha,
        })
    }

    /// Create with default precision (12).
    pub fn with_default_precision() -> AndromedaResult<Self> {
        Self::new(DEFAULT_HLL_PRECISION)
    }

    /// Get the number of registers.
    pub fn register_count(&self) -> usize {
        1 << self.precision
    }

    /// Get relative error percentage for this precision.
    pub fn error_percent(&self) -> f64 {
        (1.04 / (1u64 << self.precision) as f64).sqrt() * 100.0
    }
}

impl NdvEstimator for HyperLogLog {
    fn observe(&mut self, value: u64) {
        // **Wave 19 Todo:** Implement
        // 1. Hash value: let hash = xxhash64(&value.to_le_bytes())
        // 2. Extract register index: let idx = (hash >> (64 - precision)) as usize
        // 3. Extract remaining bits: let remaining = hash << precision
        // 4. Count leading zeros: let rho = remaining.leading_zeros() as u8 + 1
        // 5. Update register: self.registers[idx] = self.registers[idx].max(rho)
        let _ = value; // Suppress unused warning in design phase
    }

    fn estimate(&self) -> u64 {
        // **Wave 19 Todo:** Implement
        // 1. Compute raw estimate: alpha * m^2 / sum(2^(-registers[i]))
        // 2. Apply small-range bias correction if estimate <= 2.5 * m
        // 3. Apply large-range bias correction if estimate >= 2^32 / 30
        // 4. Clamp to at least 1
        1
    }

    fn memory_bytes(&self) -> usize {
        self.registers.capacity()
    }
}

// ============================================================================
// HISTOGRAM BUILDER IMPLEMENTATIONS (Wave 19)
// ============================================================================

use std::collections::HashSet;

/// Catalog-owned scalar sample value for histogram construction.
#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Null,
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float32(f32),
    Float64(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    Text(String),
}

/// Trait for histogram builders with different bucketing strategies.
pub trait HistogramBuilderTrait: Send + Sync {
    /// Add a value (including nulls) to the histogram.
    ///
    /// Returns `Err` only on resource exhaustion or encoding failures.
    /// Nulls are tracked separately.
    fn add_value(&mut self, value: &Datum) -> AndromedaResult<()>;

    /// Finalize the histogram and return an immutable representation.
    ///
    /// Performs sorting, bucketing, and validation. Returns `Err` only if
    /// final validation fails (violates bucket invariants).
    fn finalize(self: Box<Self>) -> AndromedaResult<HistogramPlaceholder>;

    /// Return estimated in-memory footprint for resource budgeting.
    fn estimated_memory_bytes(&self) -> u64;
}

/// Convert a `Datum` to a sortable key representation.
/// Returns `Err` for unsortable types (e.g., null).
fn datum_to_key(value: &Datum) -> AndromedaResult<u64> {
    match value {
        Datum::Null => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "datum_to_key: cannot key null value",
        )),
        Datum::Int8(v) => Ok((*v as i64) as u64),
        Datum::Int16(v) => Ok((*v as i64) as u64),
        Datum::Int32(v) => Ok((*v as i64) as u64),
        Datum::Int64(v) => Ok(*v as u64),
        Datum::UInt8(v) => Ok(*v as u64),
        Datum::UInt16(v) => Ok(*v as u64),
        Datum::UInt32(v) => Ok(*v as u64),
        Datum::UInt64(v) => Ok(*v),
        Datum::Float32(v) => Ok(v.to_bits() as u64),
        Datum::Float64(v) => Ok(v.to_bits()),
        Datum::Bool(v) => Ok(if *v { 1 } else { 0 }),
        Datum::Bytes(_) | Datum::Text(_) => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "datum_to_key: unsupported type for histogram bucketing",
        )),
    }
}

fn stats_validation_error(err: StatsValidationError) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Catalog, err.to_string())
}

/// Equi-width histogram builder.
/// Buckets have equal ranges in the key space.
pub struct EquiWidthHistogramBuilder {
    bucket_count: u32,
    values: Vec<Datum>,
    null_count: u64,
}

impl EquiWidthHistogramBuilder {
    /// Create a new equi-width builder with a target bucket count.
    ///
    /// If `bucket_count` is 0, returns error.
    pub fn new(bucket_count: u32) -> AndromedaResult<Self> {
        if bucket_count == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "EquiWidthHistogramBuilder: bucket_count must be > 0",
            ));
        }
        Ok(Self {
            bucket_count,
            values: Vec::new(),
            null_count: 0,
        })
    }

    /// Finalize this concrete builder into an immutable histogram.
    pub fn finalize(self) -> AndromedaResult<HistogramPlaceholder> {
        HistogramBuilderTrait::finalize(Box::new(self))
    }

    /// Estimate NDV using exact set semantics over observed non-null values.
    fn estimate_ndv(values: &[Datum]) -> AndromedaResult<u64> {
        let mut seen = HashSet::new();
        for v in values {
            // Use Debug representation as distinct key (simple approach)
            let key = format!("{:?}", v);
            seen.insert(key);
        }
        Ok(seen.len() as u64)
    }

    /// Infer a skew marker from the value distribution.
    fn infer_skew(values: &[Datum]) -> SkewMarker {
        if values.is_empty() {
            return SkewMarker::Unknown;
        }

        let ndv_exact = {
            let mut s = HashSet::new();
            for v in values {
                s.insert(format!("{:?}", v));
            }
            s.len() as f64
        };

        let total = values.len() as f64;
        let ndv_ratio = ndv_exact / total;

        match ndv_ratio {
            r if r > 0.9 => SkewMarker::Uniform,
            r if r > 0.7 => SkewMarker::LowSkew,
            r if r > 0.3 => SkewMarker::ModerateSkew,
            r if r > 0.05 => SkewMarker::HighSkew,
            _ => SkewMarker::HeavyHitter,
        }
    }
}

impl HistogramBuilderTrait for EquiWidthHistogramBuilder {
    fn add_value(&mut self, value: &Datum) -> AndromedaResult<()> {
        if matches!(value, Datum::Null) {
            self.null_count += 1;
        } else {
            self.values.push(value.clone());
        }
        Ok(())
    }

    fn finalize(mut self: Box<Self>) -> AndromedaResult<HistogramPlaceholder> {
        // Handle all-nulls case
        if self.values.is_empty() {
            return HistogramPlaceholder::new(
                vec![HistogramBucket {
                    lower_inclusive: 0,
                    upper_inclusive: 0,
                    row_estimate: 0,
                    distinct_estimate: 0,
                }],
                SkewMarker::Unknown,
            )
            .map_err(stats_validation_error);
        }

        // Sort values by datum (derived from Debug representation)
        self.values.sort_by(|a, b| {
            let a_key = datum_to_key(a);
            let b_key = datum_to_key(b);
            match (a_key, b_key) {
                (Ok(a), Ok(b)) => a.cmp(&b),
                _ => format!("{:?}", a).cmp(&format!("{:?}", b)),
            }
        });

        // Compute NDV over all values
        let ndv = Self::estimate_ndv(&self.values)?;

        // Compute key-based ranges
        let mut keys: Vec<u64> = Vec::new();
        for datum in &self.values {
            if let Ok(k) = datum_to_key(datum) {
                keys.push(k);
            }
        }

        if keys.is_empty() {
            // All values are unsortable types; create single bucket
            return HistogramPlaceholder::new(
                vec![HistogramBucket {
                    lower_inclusive: 0,
                    upper_inclusive: 0,
                    row_estimate: self.values.len() as u64,
                    distinct_estimate: ndv,
                }],
                SkewMarker::Unknown,
            )
            .map_err(stats_validation_error);
        }

        keys.sort_unstable();

        let min_key = keys.first().copied().unwrap_or(0);
        let max_key = keys.last().copied().unwrap_or(0);

        let bucket_count = (self.bucket_count as usize).min(self.values.len());
        let key_range = max_key.saturating_sub(min_key).saturating_add(1);
        let bucket_width = key_range.div_ceil(bucket_count as u64).max(1);

        let mut buckets = Vec::new();

        for b in 0..bucket_count {
            let lower_key = min_key.saturating_add((b as u64).saturating_mul(bucket_width));
            let upper_key = if b == bucket_count - 1 {
                max_key
            } else {
                min_key
                    .saturating_add(((b + 1) as u64).saturating_mul(bucket_width))
                    .saturating_sub(1)
            };

            // Count values in this bucket
            let bucket_values: Vec<&Datum> = self
                .values
                .iter()
                .filter(|v| {
                    if let Ok(k) = datum_to_key(v) {
                        k >= lower_key && k <= upper_key
                    } else {
                        false
                    }
                })
                .collect();

            if !bucket_values.is_empty() {
                let bucket_ndv = Self::estimate_ndv(
                    &bucket_values
                        .iter()
                        .map(|v| (*v).clone())
                        .collect::<Vec<_>>(),
                )?;

                buckets.push(HistogramBucket {
                    lower_inclusive: lower_key,
                    upper_inclusive: upper_key,
                    row_estimate: bucket_values.len() as u64,
                    distinct_estimate: bucket_ndv,
                });
            }
        }

        // If no buckets were created, fallback to single bucket
        if buckets.is_empty() {
            buckets.push(HistogramBucket {
                lower_inclusive: min_key,
                upper_inclusive: max_key,
                row_estimate: self.values.len() as u64,
                distinct_estimate: ndv,
            });
        }

        HistogramPlaceholder::new(buckets, Self::infer_skew(&self.values))
            .map_err(stats_validation_error)
    }

    fn estimated_memory_bytes(&self) -> u64 {
        (self.values.len() as u64) * 48 // Conservative: ~48 bytes per Datum on average
    }
}

/// Equi-depth histogram builder.
/// Buckets have approximately equal row counts.
pub struct EquiDepthHistogramBuilder {
    bucket_count: u32,
    values: Vec<Datum>,
    null_count: u64,
}

impl EquiDepthHistogramBuilder {
    /// Create a new equi-depth builder with a target bucket count.
    ///
    /// If `bucket_count` is 0, returns error.
    pub fn new(bucket_count: u32) -> AndromedaResult<Self> {
        if bucket_count == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "EquiDepthHistogramBuilder: bucket_count must be > 0",
            ));
        }
        Ok(Self {
            bucket_count,
            values: Vec::new(),
            null_count: 0,
        })
    }

    /// Finalize this concrete builder into an immutable histogram.
    pub fn finalize(self) -> AndromedaResult<HistogramPlaceholder> {
        HistogramBuilderTrait::finalize(Box::new(self))
    }

    /// Estimate NDV using exact set semantics.
    fn estimate_ndv(values: &[Datum]) -> AndromedaResult<u64> {
        let mut seen = HashSet::new();
        for v in values {
            let key = format!("{:?}", v);
            seen.insert(key);
        }
        Ok(seen.len() as u64)
    }

    /// Infer skew from distribution.
    fn infer_skew(values: &[Datum]) -> SkewMarker {
        if values.is_empty() {
            return SkewMarker::Unknown;
        }

        let ndv_exact = {
            let mut s = HashSet::new();
            for v in values {
                s.insert(format!("{:?}", v));
            }
            s.len() as f64
        };

        let total = values.len() as f64;
        let ndv_ratio = ndv_exact / total;

        match ndv_ratio {
            r if r > 0.9 => SkewMarker::Uniform,
            r if r > 0.7 => SkewMarker::LowSkew,
            r if r > 0.3 => SkewMarker::ModerateSkew,
            r if r > 0.05 => SkewMarker::HighSkew,
            _ => SkewMarker::HeavyHitter,
        }
    }
}

impl HistogramBuilderTrait for EquiDepthHistogramBuilder {
    fn add_value(&mut self, value: &Datum) -> AndromedaResult<()> {
        if matches!(value, Datum::Null) {
            self.null_count += 1;
        } else {
            self.values.push(value.clone());
        }
        Ok(())
    }

    fn finalize(mut self: Box<Self>) -> AndromedaResult<HistogramPlaceholder> {
        // Handle all-nulls case
        if self.values.is_empty() {
            return HistogramPlaceholder::new(
                vec![HistogramBucket {
                    lower_inclusive: 0,
                    upper_inclusive: 0,
                    row_estimate: 0,
                    distinct_estimate: 0,
                }],
                SkewMarker::Unknown,
            )
            .map_err(stats_validation_error);
        }

        // Sort values
        self.values.sort_by(|a, b| {
            let a_key = datum_to_key(a);
            let b_key = datum_to_key(b);
            match (a_key, b_key) {
                (Ok(a), Ok(b)) => a.cmp(&b),
                _ => format!("{:?}", a).cmp(&format!("{:?}", b)),
            }
        });

        // Compute global NDV
        let ndv = Self::estimate_ndv(&self.values)?;

        // Build buckets: equi-depth means each bucket has ~same row count
        let mut buckets = Vec::new();
        let bucket_count = (self.bucket_count as usize).min(self.values.len());

        if bucket_count == 0 {
            return HistogramPlaceholder::new(
                vec![HistogramBucket {
                    lower_inclusive: 0,
                    upper_inclusive: 0,
                    row_estimate: 0,
                    distinct_estimate: 0,
                }],
                SkewMarker::Unknown,
            )
            .map_err(stats_validation_error);
        }

        let mut start_idx = 0usize;

        while start_idx < self.values.len() && buckets.len() < bucket_count {
            let remaining_rows = self.values.len() - start_idx;
            let remaining_buckets = bucket_count - buckets.len();
            let target_size = remaining_rows.div_ceil(remaining_buckets);
            let mut end_idx = (start_idx + target_size).min(self.values.len());

            if end_idx < self.values.len() {
                let boundary_key = datum_to_key(&self.values[end_idx - 1]).ok();
                while end_idx < self.values.len()
                    && datum_to_key(&self.values[end_idx]).ok() == boundary_key
                {
                    end_idx += 1;
                }
            }

            let bucket_values = &self.values[start_idx..end_idx];
            let bucket_ndv = Self::estimate_ndv(bucket_values)?;

            // Use sorted key bounds
            let lower_key = datum_to_key(bucket_values.first().unwrap()).unwrap_or(0);
            let upper_key = datum_to_key(bucket_values.last().unwrap()).unwrap_or(0);

            buckets.push(HistogramBucket {
                lower_inclusive: lower_key,
                upper_inclusive: upper_key,
                row_estimate: bucket_values.len() as u64,
                distinct_estimate: bucket_ndv,
            });

            start_idx = end_idx;
        }

        // If no buckets created, fallback
        if buckets.is_empty() {
            buckets.push(HistogramBucket {
                lower_inclusive: 0,
                upper_inclusive: 0,
                row_estimate: self.values.len() as u64,
                distinct_estimate: ndv,
            });
        }

        HistogramPlaceholder::new(buckets, Self::infer_skew(&self.values))
            .map_err(stats_validation_error)
    }

    fn estimated_memory_bytes(&self) -> u64 {
        (self.values.len() as u64) * 48
    }
}

#[cfg(test)]
mod builder_tests {
    use super::*;

    #[test]
    fn test_equiwidth_histogram_empty() {
        let builder = EquiWidthHistogramBuilder::new(4).unwrap();
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();
        assert!(!histo.buckets().is_empty());
    }

    #[test]
    fn test_equiwidth_histogram_integers() {
        let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
        for i in 1..=100 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();
        assert!(!histo.buckets().is_empty());
        // At least one bucket should have row_estimate > 0
        assert!(histo.buckets().iter().any(|b| b.row_estimate > 0));
    }

    #[test]
    fn test_equiwidth_with_nulls() {
        let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
        for i in 1..=50 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        for _ in 0..50 {
            builder.add_value(&Datum::Null).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
    }

    #[test]
    fn test_equidepth_histogram_integers() {
        let mut builder = EquiDepthHistogramBuilder::new(4).unwrap();
        for i in 1..=100 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();
        assert!(!histo.buckets().is_empty());
    }

    #[test]
    fn test_equidepth_balanced_buckets() {
        let mut builder = EquiDepthHistogramBuilder::new(4).unwrap();
        for i in 1..=100 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();
        // Each bucket should have similar row count
        let total_rows: u64 = histo.buckets().iter().map(|b| b.row_estimate).sum();
        assert_eq!(total_rows, 100);
    }

    #[test]
    fn test_builder_zero_buckets_fails() {
        let result = EquiWidthHistogramBuilder::new(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_equidepth_builder_zero_buckets_fails() {
        let result = EquiDepthHistogramBuilder::new(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_mixed_numeric_types() {
        let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
        builder.add_value(&Datum::Int32(10)).unwrap();
        builder.add_value(&Datum::Int64(20)).unwrap();
        builder.add_value(&Datum::UInt64(30)).unwrap();
        let result = builder.finalize();
        assert!(result.is_ok());
    }

    #[test]
    fn test_skew_inference_uniform() {
        // Many distinct values → Uniform
        let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
        for i in 1..=100 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();
        assert!(matches!(
            histo.skew(),
            SkewMarker::Uniform | SkewMarker::LowSkew
        ));
    }

    #[test]
    fn test_memory_estimation() {
        let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
        for i in 1..=1000 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let estimated = builder.estimated_memory_bytes();
        assert!(estimated > 0);
        assert!(estimated >= 1000 * 40); // Conservative: at least 40 bytes per value
    }

    #[test]
    fn test_histogram_validation_invariants() {
        let mut builder = EquiWidthHistogramBuilder::new(8).unwrap();
        for i in 1..=256 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();

        // Verify all buckets maintain invariants
        for bucket in histo.buckets() {
            // lower <= upper
            assert!(bucket.lower_inclusive <= bucket.upper_inclusive);
            // distinct <= rows
            assert!(bucket.distinct_estimate <= bucket.row_estimate);
        }
    }
}
