use std::time::Instant;

use andromeda_storage_page::{
    AllocationId, InMemoryPageStore, ObjectId, PageFlags, PageHeader, PageId, PageLayoutContract,
    PageSize, PageStore, PageTrailer, PageType,
};
use andromeda_wal::Lsn;

use andromeda_bench_harness::{BenchmarkTempDir as BenchTempDir, elapsed_micros};

use crate::BenchmarkError;

pub const STORAGE_PAGE_STORE_WORKLOAD_ID: &str = "storage-page-store-smoke";
pub const STORAGE_PAGE_STORE_HARNESS_SOURCE: &str = "storage-page-owner-store";
pub const STORAGE_PAGE_STORE_HARNESS_NAME: &str = "InMemoryPageStore::allocate_page+read_page";

const FIRST_PAGE_ID: u64 = 1;
const FIRST_PAGE_LSN: u64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePageStoreSmokeBenchmark {
    pub latencies_us: Vec<u64>,
    pub flushed_pages: usize,
    pub readback_pages: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StoragePageStoreSmokeOptions {
    samples: u32,
    extent_page_count: u32,
}

pub fn run_storage_page_store_smoke_benchmark(
    samples: u32,
) -> Result<StoragePageStoreSmokeBenchmark, BenchmarkError> {
    run_storage_page_store_smoke_benchmark_with_options(StoragePageStoreSmokeOptions {
        samples,
        extent_page_count: samples,
    })
}

fn run_storage_page_store_smoke_benchmark_with_options(
    options: StoragePageStoreSmokeOptions,
) -> Result<StoragePageStoreSmokeBenchmark, BenchmarkError> {
    if options.samples == 0 {
        return Err(BenchmarkError::InsufficientSamplesForStatistics);
    }

    execute_storage_page_store_smoke(options).map_err(|_| BenchmarkError::HarnessFailed)
}

fn execute_storage_page_store_smoke(
    options: StoragePageStoreSmokeOptions,
) -> Result<StoragePageStoreSmokeBenchmark, StorageHarnessFailure> {
    let temp_dir =
        BenchTempDir::new("andromeda-bench-storage-page-store").map_err(harness_failed)?;
    let marker = temp_dir.path().join("page-store-smoke.marker");
    std::fs::write(marker, b"andromeda-bench page store smoke").map_err(harness_failed)?;

    let mut store = InMemoryPageStore::new(PageSize::KiB16);
    if options.extent_page_count < options.samples {
        return Err(StorageHarnessFailure);
    }
    let durable_lsn = Lsn::new(FIRST_PAGE_LSN + u64::from(options.samples));

    let mut latencies_us = Vec::with_capacity(options.samples as usize);
    let mut page_ids = Vec::with_capacity(options.samples as usize);
    let mut flushed_pages = 0;

    for sample in 0..options.samples {
        let page_id = PageId::new(FIRST_PAGE_ID + u64::from(sample));
        let page_lsn = Lsn::new(FIRST_PAGE_LSN + u64::from(sample));
        let started = Instant::now();

        let image = store
            .allocate_page(storage_smoke_page_contract(page_id, page_lsn), durable_lsn)
            .map_err(harness_failed)?;
        if image.page_id() != Some(page_id) || image.page_lsn() != Some(page_lsn) {
            return Err(StorageHarnessFailure);
        }

        flushed_pages += 1;
        page_ids.push(page_id);
        latencies_us.push(elapsed_micros(started));
    }

    let mut readback_pages = 0;
    for (index, page_id) in page_ids.into_iter().enumerate() {
        let expected_lsn = Lsn::new(FIRST_PAGE_LSN + index as u64);
        let image = store
            .read_page(page_id)
            .map_err(harness_failed)?
            .ok_or(StorageHarnessFailure)?;
        if image.page_id() != Some(page_id) || image.page_lsn() != Some(expected_lsn) {
            return Err(StorageHarnessFailure);
        }
        readback_pages += 1;
    }

    Ok(StoragePageStoreSmokeBenchmark {
        latencies_us,
        flushed_pages,
        readback_pages,
    })
}

fn storage_smoke_page_contract(page_id: PageId, page_lsn: Lsn) -> PageLayoutContract {
    PageLayoutContract {
        header: PageHeader {
            magic: PageHeader::MAGIC,
            format_version: PageHeader::FORMAT_VERSION_V0,
            page_size: PageSize::KiB16,
            page_type: PageType::FixedRow,
            page_id,
            object_id: ObjectId::new(1),
            allocation_id: AllocationId::new(1),
            page_lsn,
            page_epoch: 1,
            previous_page_id: None,
            next_page_id: None,
            header_len: PageHeader::MIN_HEADER_LEN_V0,
            payload_offset: 128,
            payload_len: 512,
            free_start: 256,
            free_end: 512,
            free_bytes: 256,
            slot_count: 1,
            row_count: 1,
            flags: PageFlags::NONE,
            header_crc: 5,
        },
        trailer: PageTrailer {
            payload_crc64: 6,
            page_hash: [7; 32],
            torn_write_guard: 8,
        },
    }
}

#[derive(Debug)]
struct StorageHarnessFailure;

fn harness_failed<E>(_error: E) -> StorageHarnessFailure {
    StorageHarnessFailure
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_page_store_smoke_benchmark_flushes_and_validates_readback() {
        let result = run_storage_page_store_smoke_benchmark(3).unwrap();

        assert_eq!(result.latencies_us.len(), 3);
        assert_eq!(result.flushed_pages, 3);
        assert_eq!(result.readback_pages, 3);
        assert!(result.latencies_us.iter().all(|latency| *latency >= 1));
    }

    #[test]
    fn storage_page_store_harness_failure_maps_to_benchmark_error() {
        let error =
            run_storage_page_store_smoke_benchmark_with_options(StoragePageStoreSmokeOptions {
                samples: 1,
                extent_page_count: 0,
            })
            .unwrap_err();

        assert_eq!(error, BenchmarkError::HarnessFailed);
    }
}
