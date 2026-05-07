use andromeda_types::{CatalogObjectId, CatalogVersion};

use crate::contracts::StatsVersion;

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
    let err = HistogramPlaceholder::new(vec![bucket(0, 9, 5, 6)], SkewMarker::Unknown).unwrap_err();
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
    let err =
        StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(0)).unwrap_err();
    assert_eq!(err, StatsValidationError::StatsVersionZero);
}

#[test]
fn builder_rejects_catalog_version_zero() {
    let err =
        StatsPublicationBuilder::new(CatalogVersion::new(0), StatsVersion::new(1)).unwrap_err();
    assert_eq!(err, StatsValidationError::CatalogVersionZero);
}

#[test]
fn builder_rejects_zero_object_id() {
    let err = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(0, 0), sample_histogram())
        .unwrap_err();
    assert_eq!(err, StatsValidationError::ZeroObjectId);
}

#[test]
fn builder_rejects_duplicate_target() {
    let err = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(7, 3), sample_histogram())
        .unwrap()
        .push(target(7, 3), sample_histogram())
        .unwrap_err();
    assert_eq!(err, StatsValidationError::DuplicateTarget);
}

#[test]
fn publication_digest_is_deterministic() {
    let lhs = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(7, 3), sample_histogram())
        .unwrap()
        .push(target(7, 4), sample_histogram())
        .unwrap()
        .finish();
    let rhs = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
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
    let forward = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(7, 3), sample_histogram())
        .unwrap()
        .push(target(9, 1), sample_histogram())
        .unwrap()
        .finish();
    let reversed = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(9, 1), sample_histogram())
        .unwrap()
        .push(target(7, 3), sample_histogram())
        .unwrap()
        .finish();
    assert_eq!(forward.digest(), reversed.digest());
    let forward_targets: Vec<_> = forward.entries().iter().map(|(t, _)| *t).collect();
    let reversed_targets: Vec<_> = reversed.entries().iter().map(|(t, _)| *t).collect();
    assert_eq!(forward_targets, reversed_targets);
}

#[test]
fn publication_separates_on_stats_version() {
    let v1 = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(7, 3), sample_histogram())
        .unwrap()
        .finish();
    let v2 = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(2))
        .unwrap()
        .push(target(7, 3), sample_histogram())
        .unwrap()
        .finish();
    assert_ne!(v1.digest(), v2.digest());
    assert_ne!(v1.version(), v2.version());
}

#[test]
fn publication_separates_on_catalog_version() {
    let v1 = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(7, 3), sample_histogram())
        .unwrap()
        .finish();
    let v2 = StatsPublicationBuilder::new(CatalogVersion::new(2), StatsVersion::new(1))
        .unwrap()
        .push(target(7, 3), sample_histogram())
        .unwrap()
        .finish();

    assert_ne!(v1.digest(), v2.digest());
    assert_ne!(v1.catalog_version(), v2.catalog_version());
    assert_eq!(v1.version(), v2.version());
}

#[test]
fn publication_separates_on_skew_marker() {
    let low = sample_histogram();
    let high = HistogramPlaceholder::new(low.buckets().to_vec(), SkewMarker::HighSkew).unwrap();
    let pub_low = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(7, 3), low)
        .unwrap()
        .finish();
    let pub_high = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(7, 3), high)
        .unwrap()
        .finish();
    assert_ne!(pub_low.digest(), pub_high.digest());
}

#[test]
fn publication_separates_on_bucket_evidence() {
    let base = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
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
    let altered = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(7, 3), altered_buckets)
        .unwrap()
        .finish();

    assert_ne!(base.digest(), altered.digest());
}

#[test]
fn empty_publication_is_stable_and_distinct_from_populated() {
    let empty_a = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .finish();
    let empty_b = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .finish();
    assert_eq!(empty_a.digest(), empty_b.digest());
    assert!(empty_a.is_empty());

    let populated = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .push(target(1, 0), sample_histogram())
        .unwrap()
        .finish();
    assert_ne!(empty_a.digest(), populated.digest());
}

#[test]
fn empty_publication_separates_on_stats_version() {
    let v1 = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(1))
        .unwrap()
        .finish();
    let v2 = StatsPublicationBuilder::new(CatalogVersion::new(1), StatsVersion::new(2))
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

    let lhs = StatsCorrelationPublicationBuilder::new(CatalogVersion::new(7), StatsVersion::new(3))
        .unwrap()
        .push(second.clone())
        .unwrap()
        .push(first.clone())
        .unwrap()
        .finish();

    let rhs = StatsCorrelationPublicationBuilder::new(CatalogVersion::new(7), StatsVersion::new(3))
        .unwrap()
        .push(first)
        .unwrap()
        .push(second)
        .unwrap()
        .finish();

    assert_eq!(lhs.digest(), rhs.digest());
    assert!(!lhs.digest().is_zero());
    assert_eq!(lhs.len(), 2);
    assert_eq!(lhs.entries()[0].id().get(), 1);
    assert_eq!(lhs.entries()[1].id().get(), 2);

    let mismatched = correlation(3, 8, 3, vec![target(10, 1), target(20, 2)]);
    let err = StatsCorrelationPublicationBuilder::new(CatalogVersion::new(7), StatsVersion::new(3))
        .unwrap()
        .push(mismatched)
        .unwrap_err();
    assert_eq!(err, StatsValidationError::CorrelationVersionMismatch);
}
