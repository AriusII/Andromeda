use std::time::Instant;

use andromeda_storage::{BTreeNodeV1, Lsn, PageId};

use crate::{BenchmarkError, harness::elapsed_micros};

pub const BTREE_NODE_CODEC_WORKLOAD_ID: &str = "btree-node-codec-smoke";
pub const BTREE_NODE_CODEC_HARNESS_SOURCE: &str = "storage-btree-node-v1-codec";
pub const BTREE_NODE_CODEC_HARNESS_NAME: &str = "BTreeNodeV1::encode+decode";

const PAGE_SIZE: u16 = 16 * 1024;
const FIRST_PAGE_ID: u64 = 10_000;
const FIRST_LSN: u64 = 90_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BTreeNodeCodecSmokeBenchmark {
    pub latencies_us: Vec<u64>,
    pub encoded_pages: usize,
    pub decoded_pages: usize,
}

pub fn run_btree_node_codec_smoke_benchmark(
    samples: u32,
) -> Result<BTreeNodeCodecSmokeBenchmark, BenchmarkError> {
    if samples == 0 {
        return Err(BenchmarkError::InsufficientSamplesForStatistics);
    }

    let mut latencies_us = Vec::with_capacity(samples as usize);
    let mut encoded_pages = 0usize;
    let mut decoded_pages = 0usize;

    for sample in 0..samples {
        let node = sample_node(sample).map_err(|_| BenchmarkError::HarnessFailed)?;
        let started = Instant::now();
        let encoded = node.encode().map_err(|_| BenchmarkError::HarnessFailed)?;
        encoded_pages += 1;

        let decoded = BTreeNodeV1::decode(&encoded).map_err(|_| BenchmarkError::HarnessFailed)?;
        if decoded != node {
            return Err(BenchmarkError::HarnessFailed);
        }
        decoded_pages += 1;
        latencies_us.push(elapsed_micros(started));
    }

    Ok(BTreeNodeCodecSmokeBenchmark {
        latencies_us,
        encoded_pages,
        decoded_pages,
    })
}

fn sample_node(sample: u32) -> Result<BTreeNodeV1, andromeda_core::AndromedaError> {
    if sample.is_multiple_of(2) {
        BTreeNodeV1::new_leaf(
            PageId::new(FIRST_PAGE_ID + u64::from(sample)),
            Lsn::new(FIRST_LSN + u64::from(sample)),
            leaf_entries(sample),
            previous_leaf(sample),
            next_leaf(sample),
            PAGE_SIZE,
        )
    } else {
        BTreeNodeV1::new_internal(
            PageId::new(FIRST_PAGE_ID + u64::from(sample)),
            Lsn::new(FIRST_LSN + u64::from(sample)),
            internal_keys(sample),
            internal_children(sample),
            PAGE_SIZE,
        )
    }
}

fn leaf_entries(sample: u32) -> Vec<(Vec<u8>, Vec<u8>)> {
    let base = u64::from(sample) * 100;
    (0..8)
        .map(|offset| {
            let key = (base + offset).to_be_bytes().to_vec();
            let value = format!("row-value-{sample}-{offset}").into_bytes();
            (key, value)
        })
        .collect()
}

fn internal_keys(sample: u32) -> Vec<Vec<u8>> {
    let base = u64::from(sample) * 100;
    (1..8)
        .map(|offset| (base + offset * 10).to_be_bytes().to_vec())
        .collect()
}

fn internal_children(sample: u32) -> Vec<PageId> {
    let base = FIRST_PAGE_ID + 1_000 + u64::from(sample) * 10;
    (0..8).map(|offset| PageId::new(base + offset)).collect()
}

fn previous_leaf(sample: u32) -> Option<PageId> {
    (sample > 0).then(|| PageId::new(FIRST_PAGE_ID + u64::from(sample) - 1))
}

fn next_leaf(sample: u32) -> Option<PageId> {
    Some(PageId::new(FIRST_PAGE_ID + u64::from(sample) + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn btree_node_codec_smoke_encodes_decodes_and_validates_pages() {
        let result = run_btree_node_codec_smoke_benchmark(4).unwrap();

        assert_eq!(result.latencies_us.len(), 4);
        assert_eq!(result.encoded_pages, 4);
        assert_eq!(result.decoded_pages, 4);
        assert!(result.latencies_us.iter().all(|latency| *latency >= 1));
    }

    #[test]
    fn btree_node_codec_smoke_rejects_zero_samples() {
        assert_eq!(
            run_btree_node_codec_smoke_benchmark(0).unwrap_err(),
            BenchmarkError::InsufficientSamplesForStatistics
        );
    }
}
