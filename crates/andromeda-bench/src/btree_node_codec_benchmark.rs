use std::time::Instant;

use andromeda_bench_harness::elapsed_micros;
use andromeda_bench_workload::BenchmarkError;
use andromeda_storage_page::PageId;
use andromeda_wal::Lsn;

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

        let decoded =
            BenchmarkBTreeNode::decode(&encoded).map_err(|_| BenchmarkError::HarnessFailed)?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkBTreeNode {
    page_id: PageId,
    page_lsn: Lsn,
    keys: Vec<Vec<u8>>,
    values_or_children: Vec<Vec<u8>>,
    is_leaf: bool,
}

impl BenchmarkBTreeNode {
    fn encode(&self) -> Result<Vec<u8>, BenchmarkError> {
        let mut encoded = Vec::with_capacity(PAGE_SIZE as usize);
        encoded.extend_from_slice(b"ABTN");
        encoded.push(u8::from(self.is_leaf));
        encoded.extend_from_slice(&self.page_id.get().to_le_bytes());
        encoded.extend_from_slice(&self.page_lsn.get().to_le_bytes());
        push_len(&mut encoded, self.keys.len())?;
        push_len(&mut encoded, self.values_or_children.len())?;
        for key in &self.keys {
            push_bytes(&mut encoded, key)?;
        }
        for value in &self.values_or_children {
            push_bytes(&mut encoded, value)?;
        }
        if encoded.len() > usize::from(PAGE_SIZE) {
            return Err(BenchmarkError::HarnessFailed);
        }
        encoded.resize(usize::from(PAGE_SIZE), 0);
        Ok(encoded)
    }

    fn decode(bytes: &[u8]) -> Result<Self, BenchmarkError> {
        if bytes.len() != usize::from(PAGE_SIZE) || bytes.get(0..4) != Some(b"ABTN") {
            return Err(BenchmarkError::HarnessFailed);
        }
        let is_leaf = bytes[4] == 1;
        let page_id = PageId::new(read_u64(bytes, 5)?);
        let page_lsn = Lsn::new(read_u64(bytes, 13)?);
        let mut offset = 21usize;
        let key_count = read_u16(bytes, &mut offset)? as usize;
        let value_count = read_u16(bytes, &mut offset)? as usize;
        let mut keys = Vec::with_capacity(key_count);
        for _ in 0..key_count {
            keys.push(read_bytes(bytes, &mut offset)?);
        }
        let mut values_or_children = Vec::with_capacity(value_count);
        for _ in 0..value_count {
            values_or_children.push(read_bytes(bytes, &mut offset)?);
        }
        Ok(Self {
            page_id,
            page_lsn,
            keys,
            values_or_children,
            is_leaf,
        })
    }
}

fn sample_node(sample: u32) -> Result<BenchmarkBTreeNode, BenchmarkError> {
    if sample.is_multiple_of(2) {
        let (keys, values_or_children): (Vec<_>, Vec<_>) = leaf_entries(sample).into_iter().unzip();
        Ok(BenchmarkBTreeNode {
            page_id: PageId::new(FIRST_PAGE_ID + u64::from(sample)),
            page_lsn: Lsn::new(FIRST_LSN + u64::from(sample)),
            keys,
            values_or_children,
            is_leaf: true,
        })
    } else {
        Ok(BenchmarkBTreeNode {
            page_id: PageId::new(FIRST_PAGE_ID + u64::from(sample)),
            page_lsn: Lsn::new(FIRST_LSN + u64::from(sample)),
            keys: internal_keys(sample),
            values_or_children: internal_children(sample)
                .into_iter()
                .map(|page_id| page_id.get().to_le_bytes().to_vec())
                .collect(),
            is_leaf: false,
        })
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

fn push_len(encoded: &mut Vec<u8>, len: usize) -> Result<(), BenchmarkError> {
    let len = u16::try_from(len).map_err(|_| BenchmarkError::HarnessFailed)?;
    encoded.extend_from_slice(&len.to_le_bytes());
    Ok(())
}

fn push_bytes(encoded: &mut Vec<u8>, bytes: &[u8]) -> Result<(), BenchmarkError> {
    push_len(encoded, bytes.len())?;
    encoded.extend_from_slice(bytes);
    Ok(())
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, BenchmarkError> {
    let mut value = [0u8; 8];
    value.copy_from_slice(
        bytes
            .get(offset..offset + 8)
            .ok_or(BenchmarkError::HarnessFailed)?,
    );
    Ok(u64::from_le_bytes(value))
}

fn read_u16(bytes: &[u8], offset: &mut usize) -> Result<u16, BenchmarkError> {
    let mut value = [0u8; 2];
    value.copy_from_slice(
        bytes
            .get(*offset..*offset + 2)
            .ok_or(BenchmarkError::HarnessFailed)?,
    );
    *offset += 2;
    Ok(u16::from_le_bytes(value))
}

fn read_bytes(bytes: &[u8], offset: &mut usize) -> Result<Vec<u8>, BenchmarkError> {
    let len = read_u16(bytes, offset)? as usize;
    let value = bytes
        .get(*offset..*offset + len)
        .ok_or(BenchmarkError::HarnessFailed)?
        .to_vec();
    *offset += len;
    Ok(value)
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
