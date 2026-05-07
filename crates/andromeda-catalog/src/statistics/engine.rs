use std::collections::BTreeMap;

use andromeda_types::CatalogObjectId;

use crate::contracts::StatsVersion;

use super::HistogramBucket;

pub const STATS_FULL_SCAN_THRESHOLD: u64 = 100_000;
pub const STATS_SAMPLE_SIZE: usize = 100_000;
pub const DEFAULT_BUCKET_COUNT: u32 = 32;
pub const STATS_INVALIDATION_MUTATION_PCT: f64 = 10.0;
pub const STATS_MAX_AGE_HOURS: u64 = 168;
pub const STATS_LSN_DELTA_THRESHOLD: u64 = 1_000_000;

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
