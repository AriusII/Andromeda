use andromeda_catalog::{
    DEFAULT_HLL_PRECISION, Datum, EquiDepthHistogramBuilder, EquiWidthHistogramBuilder,
    ExactNdvCounter, FeedbackStatistics, HistogramBuilderTrait, HyperLogLog, NDV_EXACT_THRESHOLD,
    NdvEstimator, PlanFeedback, SkewMarker,
};

#[test]
fn test_equiwidth_builder_creation() {
    let result = EquiWidthHistogramBuilder::new(32);
    assert!(result.is_ok());
}

#[test]
fn feedback_statistics_are_reexported_and_overflow_safe() {
    let (feedback, error_ratio) = PlanFeedback::new(u64::MAX, 0);
    assert_eq!(feedback.estimated_rows, u64::MAX);
    assert_eq!(feedback.actual_rows, 0);
    assert_eq!(error_ratio, 1.0);
    assert!(feedback.is_high_error(0.5));

    let mut stats = FeedbackStatistics::new(0.25);
    stats.record(error_ratio);
    assert_eq!(stats.total_observations, 1);
    assert_eq!(stats.average_error(), 1.0);
    assert!(stats.should_recollect());
}

#[test]
fn ndv_estimators_are_reexported_and_observe_values() {
    assert_eq!(DEFAULT_HLL_PRECISION, 12);
    assert_eq!(NDV_EXACT_THRESHOLD, 10_000);

    let mut exact = ExactNdvCounter::new();
    for value in [1, 1, 2, 3, 3, 3] {
        exact.observe(value);
    }
    assert_eq!(exact.estimate(), 3);
    assert!(exact.memory_bytes() >= 3 * std::mem::size_of::<u64>());

    let mut hll = HyperLogLog::new(10).unwrap();
    for value in 0..1_000 {
        hll.observe(value);
        hll.observe(value);
    }
    let estimate = hll.estimate();
    assert!(
        (800..=1_250).contains(&estimate),
        "HyperLogLog estimate out of expected range: {estimate}"
    );
    assert_eq!(hll.memory_bytes(), hll.register_count());
}

#[test]
fn test_equiwidth_builder_zero_buckets_rejected() {
    let result = EquiWidthHistogramBuilder::new(0);
    assert!(result.is_err());
}

#[test]
fn test_equidepth_builder_creation() {
    let result = EquiDepthHistogramBuilder::new(32);
    assert!(result.is_ok());
}

#[test]
fn test_equidepth_builder_zero_buckets_rejected() {
    let result = EquiDepthHistogramBuilder::new(0);
    assert!(result.is_err());
}

#[test]
fn test_equiwidth_all_nulls() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    for _ in 0..100 {
        builder.add_value(&Datum::Null).unwrap();
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();
    assert!(!histo.buckets().is_empty());
}

#[test]
fn test_equiwidth_int64_values() {
    let mut builder = EquiWidthHistogramBuilder::new(8).unwrap();
    for i in 1..=256 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    for bucket in histo.buckets() {
        assert!(bucket.lower_inclusive <= bucket.upper_inclusive);
        assert!(bucket.distinct_estimate <= bucket.row_estimate);
    }

    let total_rows: u64 = histo.buckets().iter().map(|b| b.row_estimate).sum();
    assert_eq!(total_rows, 256);
}

#[test]
fn test_equiwidth_mixed_null_and_values() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();

    for i in 1..=50 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
    }
    for _ in 0..50 {
        builder.add_value(&Datum::Null).unwrap();
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    let total_rows: u64 = histo.buckets().iter().map(|b| b.row_estimate).sum();
    assert_eq!(total_rows, 50);
}

#[test]
fn test_equidepth_balanced_row_distribution() {
    let mut builder = EquiDepthHistogramBuilder::new(4).unwrap();

    for i in 1..=400 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    let total_rows: u64 = histo.buckets().iter().map(|b| b.row_estimate).sum();
    assert_eq!(total_rows, 400);

    let avg_rows_per_bucket = total_rows / (histo.buckets().len() as u64);
    for bucket in histo.buckets() {
        let deviation = ((bucket.row_estimate as i64 - avg_rows_per_bucket as i64).abs() as f64
            / avg_rows_per_bucket as f64)
            * 100.0;
        assert!(
            deviation <= 50.0,
            "Bucket deviation too high: {}%",
            deviation
        );
    }
}

#[test]
fn test_equidepth_single_bucket() {
    let mut builder = EquiDepthHistogramBuilder::new(1).unwrap();

    for i in 1..=100 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    assert_eq!(histo.buckets().len(), 1);
    assert_eq!(histo.buckets()[0].row_estimate, 100);
}

#[test]
fn test_mixed_numeric_types() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();

    builder.add_value(&Datum::Int8(5)).unwrap();
    builder.add_value(&Datum::Int16(100)).unwrap();
    builder.add_value(&Datum::Int32(10000)).unwrap();
    builder.add_value(&Datum::Int64(1000000)).unwrap();
    builder.add_value(&Datum::UInt8(42)).unwrap();
    builder
        .add_value(&Datum::Float64(std::f64::consts::PI))
        .unwrap();

    let result = builder.finalize();
    assert!(result.is_ok());
}

#[test]
fn test_skew_inference_uniform() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();

    for i in 1..=200 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
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
fn test_skew_inference_heavy_hitter() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();

    for _ in 0..900 {
        builder.add_value(&Datum::Int64(1)).unwrap();
    }
    for i in 2..=11 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    assert!(matches!(
        histo.skew(),
        SkewMarker::HighSkew | SkewMarker::HeavyHitter
    ));
}

#[test]
fn test_memory_estimation_grows_with_count() {
    let mut builder1 = EquiWidthHistogramBuilder::new(4).unwrap();
    for i in 1..=100 {
        builder1.add_value(&Datum::Int64(i as i64)).unwrap();
    }
    let mem1 = builder1.estimated_memory_bytes();

    let mut builder2 = EquiWidthHistogramBuilder::new(4).unwrap();
    for i in 1..=1000 {
        builder2.add_value(&Datum::Int64(i as i64)).unwrap();
    }
    let mem2 = builder2.estimated_memory_bytes();

    assert!(mem2 > mem1);
    assert_eq!(mem2 / mem1, 10);
}

#[test]
fn test_boolean_values() {
    let mut builder = EquiWidthHistogramBuilder::new(2).unwrap();

    for _ in 0..60 {
        builder.add_value(&Datum::Bool(true)).unwrap();
    }
    for _ in 0..40 {
        builder.add_value(&Datum::Bool(false)).unwrap();
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    let total_rows: u64 = histo.buckets().iter().map(|b| b.row_estimate).sum();
    assert_eq!(total_rows, 100);
}

#[test]
fn test_duplicate_values_ndv_estimation() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();

    for i in 0..10 {
        for _ in 0..100 {
            builder.add_value(&Datum::Int64(i as i64)).unwrap();
        }
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    assert!(
        histo
            .buckets()
            .iter()
            .map(|b| b.distinct_estimate)
            .sum::<u64>()
            <= 20
    );
}

#[test]
fn test_float_values() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();

    builder.add_value(&Datum::Float32(1.0)).unwrap();
    builder.add_value(&Datum::Float32(2.5)).unwrap();
    builder
        .add_value(&Datum::Float32(std::f32::consts::PI))
        .unwrap();
    builder.add_value(&Datum::Float64(100.0)).unwrap();
    builder.add_value(&Datum::Float64(-50.5)).unwrap();

    let result = builder.finalize();
    assert!(result.is_ok());
}

#[test]
fn test_large_value_count() {
    let mut builder = EquiDepthHistogramBuilder::new(32).unwrap();

    for i in 0..10_000 {
        builder.add_value(&Datum::Int64(i as i64 % 100)).unwrap();
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    let total_rows: u64 = histo.buckets().iter().map(|b| b.row_estimate).sum();
    assert_eq!(total_rows, 10_000);
}

#[test]
fn test_bucket_monotonicity() {
    let mut builder = EquiWidthHistogramBuilder::new(16).unwrap();

    for i in 1..=512 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    let buckets = histo.buckets();
    for i in 0..buckets.len().saturating_sub(1) {
        assert!(
            buckets[i].upper_inclusive < buckets[i + 1].lower_inclusive,
            "Buckets {} and {} violate monotonicity",
            i,
            i + 1
        );
    }
}

#[test]
fn test_single_value() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    builder.add_value(&Datum::Int64(42)).unwrap();

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    let total_rows: u64 = histo.buckets().iter().map(|b| b.row_estimate).sum();
    assert_eq!(total_rows, 1);
}

#[test]
fn test_equiwidth_bucket_count_clamping() {
    let mut builder = EquiWidthHistogramBuilder::new(1000).unwrap();

    for i in 1..=10 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
    }

    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();

    assert!(histo.buckets().len() <= 10);
}
