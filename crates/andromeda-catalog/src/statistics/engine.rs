use std::collections::BTreeMap;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId};

use crate::contracts::StatsVersion;

use super::HistogramBucket;

pub const STATS_FULL_SCAN_THRESHOLD: u64 = 100_000;
pub const STATS_SAMPLE_SIZE: usize = 100_000;
pub const DEFAULT_BUCKET_COUNT: u32 = 32;
pub const DEFAULT_HLL_PRECISION: u8 = 12;
pub const STATS_INVALIDATION_MUTATION_PCT: f64 = 10.0;
pub const STATS_MAX_AGE_HOURS: u64 = 168;
pub const STATS_LSN_DELTA_THRESHOLD: u64 = 1_000_000;
pub const NDV_EXACT_THRESHOLD: u64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HistogramAlgorithm {
    EquiWidth,
    EquiDepth,
}

impl HistogramAlgorithm {
    pub fn name(&self) -> &'static str {
        match self {
            HistogramAlgorithm::EquiWidth => "equi-width",
            HistogramAlgorithm::EquiDepth => "equi-depth",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquiWidthHistogram {
    pub column_id: u64,
    pub bucket_count: u32,
    pub min_value: u64,
    pub max_value: u64,
    pub buckets: Vec<HistogramBucket>,
    pub null_count: u64,
    pub ndv_estimate: u64,
}

impl EquiWidthHistogram {
    pub fn total_rows(&self) -> u64 {
        self.buckets.iter().map(|b| b.row_estimate).sum()
    }

    pub fn total_distinct(&self) -> u64 {
        self.ndv_estimate
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquiDepthHistogram {
    pub column_id: u64,
    pub bucket_count: u32,
    pub min_value: u64,
    pub max_value: u64,
    pub buckets: Vec<HistogramBucket>,
    pub null_count: u64,
    pub ndv_estimate: u64,
}

impl EquiDepthHistogram {
    pub fn total_rows(&self) -> u64 {
        self.buckets.iter().map(|b| b.row_estimate).sum()
    }

    pub fn total_distinct(&self) -> u64 {
        self.ndv_estimate
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptiveHistogram {
    pub column_id: u64,
    pub bucket_count: u32,
    pub min_value: u64,
    pub max_value: u64,
    pub buckets: Vec<HistogramBucket>,
    pub null_count: u64,
    pub ndv_estimate: u64,
    pub algorithm_choice: HistogramAlgorithm,
}

impl AdaptiveHistogram {
    pub fn total_rows(&self) -> u64 {
        self.buckets.iter().map(|b| b.row_estimate).sum()
    }

    pub fn total_distinct(&self) -> u64 {
        self.ndv_estimate
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Histogram {
    EquiWidth(EquiWidthHistogram),
    EquiDepth(EquiDepthHistogram),
    Adaptive(AdaptiveHistogram),
}

impl Histogram {
    pub fn column_id(&self) -> u64 {
        match self {
            Histogram::EquiWidth(h) => h.column_id,
            Histogram::EquiDepth(h) => h.column_id,
            Histogram::Adaptive(h) => h.column_id,
        }
    }

    pub fn bucket_count(&self) -> u32 {
        match self {
            Histogram::EquiWidth(h) => h.bucket_count,
            Histogram::EquiDepth(h) => h.bucket_count,
            Histogram::Adaptive(h) => h.bucket_count,
        }
    }

    pub fn total_rows(&self) -> u64 {
        match self {
            Histogram::EquiWidth(h) => h.total_rows(),
            Histogram::EquiDepth(h) => h.total_rows(),
            Histogram::Adaptive(h) => h.total_rows(),
        }
    }

    pub fn total_distinct(&self) -> u64 {
        match self {
            Histogram::EquiWidth(h) => h.total_distinct(),
            Histogram::EquiDepth(h) => h.total_distinct(),
            Histogram::Adaptive(h) => h.total_distinct(),
        }
    }

    pub fn algorithm(&self) -> HistogramAlgorithm {
        match self {
            Histogram::EquiWidth(_) => HistogramAlgorithm::EquiWidth,
            Histogram::EquiDepth(_) => HistogramAlgorithm::EquiDepth,
            Histogram::Adaptive(h) => h.algorithm_choice,
        }
    }

    pub fn buckets(&self) -> &[HistogramBucket] {
        match self {
            Histogram::EquiWidth(h) => &h.buckets,
            Histogram::EquiDepth(h) => &h.buckets,
            Histogram::Adaptive(h) => &h.buckets,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnStatistics {
    pub column_id: u64,
    pub histogram: Histogram,
    pub null_count: u64,
    pub ndv: u64,
    pub collected_at_lsn: u64,
}

impl ColumnStatistics {
    pub fn non_null_rows(&self) -> u64 {
        self.histogram.total_rows()
    }

    pub fn total_rows_with_nulls(&self) -> u64 {
        self.histogram.total_rows() + self.null_count
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableStatistics {
    pub table_id: CatalogObjectId,
    pub column_stats: BTreeMap<u16, ColumnStatistics>,
    pub row_count: u64,
    pub collected_at_lsn: u64,
    pub version: StatsVersion,
}

impl TableStatistics {
    pub fn add_column_stats(&mut self, col_index: u16, stats: ColumnStatistics) {
        self.column_stats.insert(col_index, stats);
    }

    pub fn get_column_stats(&self, col_index: u16) -> Option<&ColumnStatistics> {
        self.column_stats.get(&col_index)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatsInvalidationPolicy {
    pub mutation_threshold_percent: f64,
    pub max_age_hours: u64,
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

pub trait HistogramBuilder: Send + Sync {
    fn add_value(&mut self, value: u64) -> andromeda_core::AndromedaResult<()>;
    fn finalize(self: Box<Self>) -> andromeda_core::AndromedaResult<Histogram>;
    fn estimated_memory_bytes(&self) -> usize;
}

pub trait NdvEstimator: Send + Sync {
    fn observe(&mut self, value: u64);
    fn estimate(&self) -> u64;
    fn memory_bytes(&self) -> usize;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanFeedback {
    pub estimated_rows: u64,
    pub actual_rows: u64,
}

impl PlanFeedback {
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

#[derive(Debug, Clone, PartialEq)]
pub struct FeedbackStatistics {
    pub total_observations: u64,
    pub sum_errors: f64,
    pub max_error: f64,
    pub high_error_threshold: f64,
}

impl FeedbackStatistics {
    pub fn new(high_error_threshold: f64) -> Self {
        Self {
            total_observations: 0,
            sum_errors: 0.0,
            max_error: 0.0,
            high_error_threshold,
        }
    }

    pub fn record(&mut self, error_ratio: f64) {
        self.total_observations += 1;
        self.sum_errors += error_ratio;
        self.max_error = self.max_error.max(error_ratio);
    }

    pub fn average_error(&self) -> f64 {
        if self.total_observations > 0 {
            self.sum_errors / self.total_observations as f64
        } else {
            0.0
        }
    }

    pub fn should_recollect(&self) -> bool {
        self.max_error > self.high_error_threshold
            || self.average_error() > self.high_error_threshold
    }
}

#[derive(Debug, Clone)]
pub struct ExactNdvCounter {
    seen: std::collections::HashSet<u64>,
}

impl ExactNdvCounter {
    pub fn new() -> Self {
        Self {
            seen: std::collections::HashSet::new(),
        }
    }

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

#[derive(Debug, Clone)]
pub struct HyperLogLog {
    pub precision: u8,
    pub registers: Vec<u8>,
    pub alpha: f64,
}

impl HyperLogLog {
    pub fn new(precision: u8) -> AndromedaResult<Self> {
        if !(4..=16).contains(&precision) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
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

    pub fn with_default_precision() -> AndromedaResult<Self> {
        Self::new(DEFAULT_HLL_PRECISION)
    }

    pub fn register_count(&self) -> usize {
        1 << self.precision
    }

    pub fn error_percent(&self) -> f64 {
        (1.04 / (1u64 << self.precision) as f64).sqrt() * 100.0
    }
}

impl NdvEstimator for HyperLogLog {
    fn observe(&mut self, value: u64) {
        let _ = value;
    }

    fn estimate(&self) -> u64 {
        1
    }

    fn memory_bytes(&self) -> usize {
        self.registers.capacity()
    }
}
