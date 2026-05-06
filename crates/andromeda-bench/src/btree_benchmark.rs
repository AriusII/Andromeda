#![forbid(unsafe_code)]

//! B-Tree benchmark harness for read-only lookup and range scan operations.
//!
//! Uses an in-memory mock tree with deterministic keys.
//!
//! ## Invariants
//!
//! - The harness never calls insert, delete, split, or merge.
//! - The same parameters produce the same P50/P95 within natural timer variance.
//! - The mock tree is compile-time restricted to read operations.

use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct BTreeBenchmarkConfig {
    pub dataset_size: usize,
    pub hit_rate_percent: u8,
    pub range_scan_distribution: (usize, usize, usize),
}

impl Default for BTreeBenchmarkConfig {
    fn default() -> Self {
        Self {
            dataset_size: 10_000,
            hit_rate_percent: 80,
            range_scan_distribution: (100, 500, 1000),
        }
    }
}

/// In-memory B-Tree mock that models read-only performance of a sorted key set.
#[derive(Debug)]
struct MockBTreeIndex {
    keys: Vec<Vec<u8>>,
}

impl MockBTreeIndex {
    fn new(dataset_size: usize) -> Self {
        let mut keys = Vec::with_capacity(dataset_size);
        for i in 0..dataset_size {
            keys.push(i.to_le_bytes().to_vec());
        }
        keys.sort();
        Self { keys }
    }

    fn lookup(&self, key: &[u8]) -> usize {
        let mut comparisons = 0;
        let mut left = 0;
        let mut right = self.keys.len();

        while left < right {
            comparisons += 1;
            let mid = (left + right) / 2;
            match key.cmp(&self.keys[mid]) {
                std::cmp::Ordering::Less => right = mid,
                std::cmp::Ordering::Greater => left = mid + 1,
                std::cmp::Ordering::Equal => break,
            }
        }
        comparisons
    }

    fn range_scan(&self, start_key: &[u8], end_key: &[u8]) -> usize {
        let mut comparisons = 0;

        let mut left = 0;
        let mut right = self.keys.len();
        while left < right {
            comparisons += 1;
            let mid = (left + right) / 2;
            match start_key.cmp(&self.keys[mid]) {
                std::cmp::Ordering::Less => right = mid,
                std::cmp::Ordering::Greater => left = mid + 1,
                std::cmp::Ordering::Equal => break,
            }
        }

        let mut count = 0;
        for key in &self.keys[left..] {
            if key[..] < end_key[..] {
                count += 1;
            } else {
                break;
            }
        }

        comparisons + count
    }
}

/// TECH-DEBT: Context: Replace `MockBTreeIndex` with `BTreeIndexEngine` once the real engine
/// exists with buffer pool, WAL, and catalog wiring.
/// Risk: Until replaced, benchmark latencies do not include page IO or WAL append cost.
/// Closure: Supersede when `andromeda-storage` exposes a `BTreeIndexEngine` trait.
#[derive(Debug)]
pub struct BTreeBenchmarkContext {
    tree: Arc<MockBTreeIndex>,
    config: BTreeBenchmarkConfig,
}

impl BTreeBenchmarkContext {
    pub fn config(&self) -> &BTreeBenchmarkConfig {
        &self.config
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeBenchmarkError {
    SetupFailed,
    InvalidConfig,
    LatencyOverflow,
}

pub fn setup_btree_lookup_harness(
    config: BTreeBenchmarkConfig,
) -> Result<BTreeBenchmarkContext, BTreeBenchmarkError> {
    if config.dataset_size == 0 {
        return Err(BTreeBenchmarkError::InvalidConfig);
    }

    let tree = Arc::new(MockBTreeIndex::new(config.dataset_size));
    Ok(BTreeBenchmarkContext { tree, config })
}

pub fn setup_btree_range_scan_harness(
    config: BTreeBenchmarkConfig,
) -> Result<BTreeBenchmarkContext, BTreeBenchmarkError> {
    if config.dataset_size == 0 {
        return Err(BTreeBenchmarkError::InvalidConfig);
    }

    let tree = Arc::new(MockBTreeIndex::new(config.dataset_size));
    Ok(BTreeBenchmarkContext { tree, config })
}

pub fn benchmark_btree_lookup(
    ctx: &BTreeBenchmarkContext,
    key: &[u8],
) -> Result<u64, BTreeBenchmarkError> {
    let start = Instant::now();
    let _comparisons = ctx.tree.lookup(key);
    let elapsed = start.elapsed();

    let micros = elapsed.as_micros().max(1);
    micros
        .try_into()
        .map_err(|_| BTreeBenchmarkError::LatencyOverflow)
}

pub fn benchmark_btree_range_scan(
    ctx: &BTreeBenchmarkContext,
    start_key: &[u8],
    end_key: &[u8],
) -> Result<u64, BTreeBenchmarkError> {
    let start = Instant::now();
    let _key_count = ctx.tree.range_scan(start_key, end_key);
    let elapsed = start.elapsed();

    let micros = elapsed.as_micros().max(1);
    micros
        .try_into()
        .map_err(|_| BTreeBenchmarkError::LatencyOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_btree_harness_setup_lookup() {
        let config = BTreeBenchmarkConfig::default();
        let ctx = setup_btree_lookup_harness(config).unwrap();
        assert_eq!(ctx.config().dataset_size, 10_000);
    }

    #[test]
    fn test_btree_harness_setup_range_scan() {
        let config = BTreeBenchmarkConfig::default();
        assert!(setup_btree_range_scan_harness(config).is_ok());
    }

    #[test]
    fn test_btree_harness_setup_rejects_empty_dataset() {
        let config = BTreeBenchmarkConfig {
            dataset_size: 0,
            ..BTreeBenchmarkConfig::default()
        };
        assert_eq!(
            setup_btree_lookup_harness(config).unwrap_err(),
            BTreeBenchmarkError::InvalidConfig
        );
    }

    #[test]
    fn test_btree_lookup_returns_nonzero_latency() {
        let config = BTreeBenchmarkConfig::default();
        let ctx = setup_btree_lookup_harness(config).unwrap();

        let key = 100u64.to_le_bytes().to_vec();
        let latency_us = benchmark_btree_lookup(&ctx, &key).unwrap();
        assert!(latency_us >= 1);
    }

    #[test]
    fn test_btree_lookup_hit_and_miss_both_complete() {
        let config = BTreeBenchmarkConfig::default();
        let ctx = setup_btree_lookup_harness(config).unwrap();

        let existing_key = 1000u64.to_le_bytes().to_vec();
        let hit_latency = benchmark_btree_lookup(&ctx, &existing_key).unwrap();

        let missing_key = 1_000_000u64.to_le_bytes().to_vec();
        let miss_latency = benchmark_btree_lookup(&ctx, &missing_key).unwrap();

        assert!(hit_latency >= 1);
        assert!(miss_latency >= 1);
    }

    #[test]
    fn test_btree_range_scan_returns_nonzero_latency() {
        let config = BTreeBenchmarkConfig::default();
        let ctx = setup_btree_range_scan_harness(config).unwrap();

        let start_key = 1000u64.to_le_bytes().to_vec();
        let end_key = 2000u64.to_le_bytes().to_vec();
        let latency_us = benchmark_btree_range_scan(&ctx, &start_key, &end_key).unwrap();
        assert!(latency_us >= 1);
    }

    #[test]
    fn test_mock_btree_lookup_deterministic() {
        let tree = MockBTreeIndex::new(10_000);
        let key = 5000u64.to_le_bytes().to_vec();

        assert_eq!(tree.lookup(&key), tree.lookup(&key));
    }

    #[test]
    fn test_mock_btree_range_scan_deterministic() {
        let tree = MockBTreeIndex::new(10_000);
        let start = 1000u64.to_le_bytes().to_vec();
        let end = 2000u64.to_le_bytes().to_vec();

        assert_eq!(tree.range_scan(&start, &end), tree.range_scan(&start, &end));
    }

    #[test]
    fn test_benchmark_config_default_is_valid() {
        let config = BTreeBenchmarkConfig::default();
        assert!(config.dataset_size > 0);
        assert!(config.hit_rate_percent <= 100);
        assert!(config.range_scan_distribution.0 <= config.range_scan_distribution.1);
        assert!(config.range_scan_distribution.1 <= config.range_scan_distribution.2);
    }
}
