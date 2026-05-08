use crate::SkewMarker;

use super::{Datum, EquiDepthHistogramBuilder, EquiWidthHistogramBuilder, HistogramBuilderTrait};

fn add_int64_range<B>(builder: &mut B, start: i64, end: i64)
where
    B: HistogramBuilderTrait,
{
    for value in start..=end {
        builder.add_value(&Datum::Int64(value)).unwrap();
    }
}

fn add_nulls<B>(builder: &mut B, count: usize)
where
    B: HistogramBuilderTrait,
{
    for _ in 0..count {
        builder.add_value(&Datum::Null).unwrap();
    }
}

#[test]
fn equiwidth_histogram_empty() {
    let histo = EquiWidthHistogramBuilder::new(4)
        .unwrap()
        .finalize()
        .unwrap();

    assert_eq!(histo.buckets().len(), 1);
    assert_eq!(histo.buckets()[0].row_estimate, 0);
    assert_eq!(histo.skew(), SkewMarker::Unknown);
}

#[test]
fn equiwidth_histogram_integers() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    add_int64_range(&mut builder, 1, 100);

    let histo = builder.finalize().unwrap();

    assert!(!histo.buckets().is_empty());
    assert!(histo.buckets().iter().any(|bucket| bucket.row_estimate > 0));
}

#[test]
fn equiwidth_omits_nulls_from_bucket_rows() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    add_int64_range(&mut builder, 1, 50);
    add_nulls(&mut builder, 50);

    let histo = builder.finalize().unwrap();
    let total_rows: u64 = histo
        .buckets()
        .iter()
        .map(|bucket| bucket.row_estimate)
        .sum();

    assert_eq!(total_rows, 50);
}

#[test]
fn equidepth_histogram_integers() {
    let mut builder = EquiDepthHistogramBuilder::new(4).unwrap();
    add_int64_range(&mut builder, 1, 100);

    let histo = builder.finalize().unwrap();

    assert!(!histo.buckets().is_empty());
}

#[test]
fn equidepth_balanced_buckets_keep_all_rows() {
    let mut builder = EquiDepthHistogramBuilder::new(4).unwrap();
    add_int64_range(&mut builder, 1, 100);

    let histo = builder.finalize().unwrap();
    let total_rows: u64 = histo
        .buckets()
        .iter()
        .map(|bucket| bucket.row_estimate)
        .sum();

    assert_eq!(total_rows, 100);
}

#[test]
fn builders_reject_zero_buckets() {
    assert!(EquiWidthHistogramBuilder::new(0).is_err());
    assert!(EquiDepthHistogramBuilder::new(0).is_err());
}

#[test]
fn equiwidth_accepts_mixed_numeric_types() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    builder.add_value(&Datum::Int32(10)).unwrap();
    builder.add_value(&Datum::Int64(20)).unwrap();
    builder.add_value(&Datum::UInt64(30)).unwrap();

    assert!(builder.finalize().is_ok());
}

#[test]
fn skew_inference_uniform() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    add_int64_range(&mut builder, 1, 100);

    let histo = builder.finalize().unwrap();

    assert!(matches!(
        histo.skew(),
        SkewMarker::Uniform | SkewMarker::LowSkew
    ));
}

#[test]
fn equiwidth_repeated_values_expose_density_and_heavy_hitter_skew() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    for _ in 0..97 {
        builder.add_value(&Datum::Int64(7)).unwrap();
    }
    add_int64_range(&mut builder, 8, 10);

    let histo = builder.finalize().unwrap();
    let dense_bucket = histo
        .buckets()
        .iter()
        .find(|bucket| bucket.lower_inclusive <= 7 && bucket.upper_inclusive >= 7)
        .expect("histogram should retain a bucket for the repeated value");

    assert_eq!(histo.skew(), SkewMarker::HeavyHitter);
    assert!(dense_bucket.row_estimate >= 97);
    assert!(dense_bucket.distinct_estimate <= 2);
    assert!(
        dense_bucket.distinct_estimate.saturating_mul(10) < dense_bucket.row_estimate,
        "dense repeated-value bucket should expose low distinct-per-row evidence"
    );
}

#[test]
fn memory_estimation_tracks_non_null_values() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    add_int64_range(&mut builder, 1, 1000);
    add_nulls(&mut builder, 1000);

    let estimated = builder.estimated_memory_bytes();

    assert!(estimated > 0);
    assert_eq!(estimated, 1000 * 48);
}

#[test]
fn histogram_validation_invariants_hold() {
    let mut builder = EquiWidthHistogramBuilder::new(8).unwrap();
    add_int64_range(&mut builder, 1, 256);

    let histo = builder.finalize().unwrap();

    for bucket in histo.buckets() {
        assert!(bucket.lower_inclusive <= bucket.upper_inclusive);
        assert!(bucket.distinct_estimate <= bucket.row_estimate);
    }
}

#[test]
fn equidepth_unsupported_values_stay_bounded() {
    let mut builder = EquiDepthHistogramBuilder::new(4).unwrap();
    builder
        .add_value(&Datum::Text("alpha".to_string()))
        .unwrap();
    builder.add_value(&Datum::Text("beta".to_string())).unwrap();

    let histo = builder.finalize().unwrap();

    assert_eq!(histo.buckets().len(), 1);
    assert_eq!(histo.buckets()[0].row_estimate, 2);
    assert_eq!(histo.buckets()[0].distinct_estimate, 2);
    assert_eq!(histo.skew(), SkewMarker::Unknown);
}
