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

use andromeda_core::CatalogObjectId;

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

/// Domain tag absorbed at the start of every publication digest.
const STATS_PUBLICATION_DOMAIN: &[u8] = b"andromeda.stats.publication.v0";

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
            if let Some(prev) = prev_upper {
                if bucket.lower_inclusive <= prev {
                    return Err(StatsValidationError::BucketsNotMonotonic);
                }
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
}
