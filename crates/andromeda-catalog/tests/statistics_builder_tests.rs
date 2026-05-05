//! Integration tests for statistics histogram builders.
//!
//! Tests verify:
//! - Builder construction and finalization
//! - Equi-width and equi-depth histogram properties
//! - NDV estimation accuracy
//! - Null value handling
//! - Histogram invariant validation
//! - Memory estimation

use andromeda_catalog::{
    EquiWidthHistogramBuilder, EquiDepthHistogramBuilder, HistogramBuilderTrait,
    SkewMarker,
};
use andromeda_storage::heap_row_encoder::Datum;

#[test]
fn test_equiwidth_builder_creation() {
    let result = EquiWidthHistogramBuilder::new(32);
    assert!(result.is_ok());
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
    assert!(histo.buckets().len() > 0);
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
    
    // Verify histogram invariants
    for bucket in histo.buckets() {
        // Each bucket must satisfy lower <= upper
        assert!(bucket.lower_inclusive <= bucket.upper_inclusive);
        // distinct <= rows
        assert!(bucket.distinct_estimate <= bucket.row_estimate);
    }
    
    // Sum of row counts should equal input count
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
    
    // For equi-depth, each bucket should have ~same row count
    let total_rows: u64 = histo.buckets().iter().map(|b| b.row_estimate).sum();
    assert_eq!(total_rows, 400);
    
    let avg_rows_per_bucket = total_rows / (histo.buckets().len() as u64);
    for bucket in histo.buckets() {
        // Allow up to 50% deviation from average
        let deviation = ((bucket.row_estimate as i64 - avg_rows_per_bucket as i64).abs()
            as f64
            / avg_rows_per_bucket as f64)
            * 100.0;
        assert!(deviation <= 50.0, "Bucket deviation too high: {}%", deviation);
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
    builder.add_value(&Datum::Float64(3.14159)).unwrap();
    
    let result = builder.finalize();
    assert!(result.is_ok());
}

#[test]
fn test_skew_inference_uniform() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    
    // Uniform distribution: many distinct values
    for i in 1..=200 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
    }
    
    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();
    
    // With high NDV ratio, should infer Uniform or LowSkew
    assert!(matches!(
        histo.skew(),
        SkewMarker::Uniform | SkewMarker::LowSkew
    ));
}

#[test]
fn test_skew_inference_heavy_hitter() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    
    // Heavy hitter: one value repeated many times, few others
    for _ in 0..900 {
        builder.add_value(&Datum::Int64(1)).unwrap();
    }
    for i in 2..=11 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
    }
    
    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();
    
    // With very low NDV ratio, should infer HighSkew or HeavyHitter
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
    
    // Memory should grow with value count
    assert!(mem2 > mem1);
    assert_eq!(mem2 / mem1, 10); // Should be roughly 10x for 10x data
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
    
    // 1000 rows, only 10 distinct values
    for i in 0..10 {
        for _ in 0..100 {
            builder.add_value(&Datum::Int64(i as i64)).unwrap();
        }
    }
    
    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();
    
    // NDV should be approximately 10
    assert!(histo.buckets().iter().map(|b| b.distinct_estimate).sum::<u64>() <= 20);
}

#[test]
fn test_float_values() {
    let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
    
    builder.add_value(&Datum::Float32(1.0)).unwrap();
    builder.add_value(&Datum::Float32(2.5)).unwrap();
    builder.add_value(&Datum::Float32(3.14159)).unwrap();
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
    
    // Verify strict monotonicity: each bucket's upper < next bucket's lower
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
    // Request more buckets than available values
    let mut builder = EquiWidthHistogramBuilder::new(1000).unwrap();
    
    for i in 1..=10 {
        builder.add_value(&Datum::Int64(i as i64)).unwrap();
    }
    
    let result = builder.finalize();
    assert!(result.is_ok());
    let histo = result.unwrap();
    
    // Bucket count should be clamped to value count
    assert!(histo.buckets().len() <= 10);
}
