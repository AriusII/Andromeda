use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct BufferPoolMetricsAccumulator {
    cache_hits: u64,
    cache_misses: u64,
    eviction_stalls: u64,
    flush_stalls: u64,
}

impl BufferPoolMetricsAccumulator {
    pub const fn new() -> Self {
        Self {
            cache_hits: 0,
            cache_misses: 0,
            eviction_stalls: 0,
            flush_stalls: 0,
        }
    }

    pub fn record_hit(&mut self) {
        self.cache_hits = self.cache_hits.saturating_add(1);
    }

    pub fn record_miss(&mut self) {
        self.cache_misses = self.cache_misses.saturating_add(1);
    }

    pub fn record_eviction_stall(&mut self) {
        self.eviction_stalls = self.eviction_stalls.saturating_add(1);
    }

    pub fn record_flush_stall(&mut self) {
        self.flush_stalls = self.flush_stalls.saturating_add(1);
    }

    pub fn snapshot(
        self,
        frame_capacity: usize,
        resident_pages: usize,
        dirty_pages: usize,
    ) -> BufferPoolMetrics {
        BufferPoolMetrics {
            frame_capacity,
            resident_pages,
            dirty_pages,
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            eviction_stalls: self.eviction_stalls,
            flush_stalls: self.flush_stalls,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPoolMetrics {
    pub frame_capacity: usize,
    pub resident_pages: usize,
    pub dirty_pages: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub eviction_stalls: u64,
    pub flush_stalls: u64,
}

impl BufferPoolMetrics {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.frame_capacity == 0 {
            return Err(storage_error(
                "buffer pool metrics require a non-zero frame capacity",
            ));
        }
        if self.resident_pages > self.frame_capacity {
            return Err(storage_error(
                "buffer pool resident page count cannot exceed frame capacity",
            ));
        }
        if self.dirty_pages > self.resident_pages {
            return Err(storage_error(
                "buffer pool dirty page count cannot exceed resident pages",
            ));
        }
        Ok(())
    }

    pub const fn total_accesses(self) -> u64 {
        self.cache_hits + self.cache_misses
    }

    pub const fn total_stalls(self) -> u64 {
        self.eviction_stalls + self.flush_stalls
    }

    pub const fn has_stalls(self) -> bool {
        self.total_stalls() != 0
    }

    pub fn hit_ratio(self) -> Option<f64> {
        let total = self.total_accesses();
        (total != 0).then_some(self.cache_hits as f64 / total as f64)
    }

    pub fn dirty_ratio(self) -> Option<f64> {
        (self.resident_pages != 0).then_some(self.dirty_pages as f64 / self.resident_pages as f64)
    }
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use andromeda_storage_page::{
        AllocationId, InMemoryPageStore, Lsn, ObjectId, PageFlags, PageHeader, PageId,
        PageLayoutContract, PageSize, PageStore, PageTrailer, PageType,
    };

    use andromeda_error::AndromedaErrorKind;

    use crate::{BufferPool, BufferPoolConfig, BufferPoolManager, TestWalDurabilityObserver};

    use super::*;

    fn valid_contract(page_id: PageId, page_lsn: Lsn) -> PageLayoutContract {
        PageLayoutContract {
            header: PageHeader {
                magic: PageHeader::MAGIC,
                format_version: PageHeader::FORMAT_VERSION_V0,
                page_size: PageSize::KiB16,
                page_type: PageType::FixedRow,
                page_id,
                object_id: ObjectId::new(2),
                allocation_id: AllocationId::new(3),
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

    fn store_with_pages(pages: &[(PageId, Lsn)]) -> InMemoryPageStore {
        let mut store = InMemoryPageStore::new(PageSize::KiB16);
        for (page_id, page_lsn) in pages {
            store
                .allocate_page(valid_contract(*page_id, *page_lsn), *page_lsn)
                .expect("test page allocation is valid");
        }
        store
    }

    #[test]
    fn metrics_validate_buffer_pool_pressure_snapshot() {
        let metrics = BufferPoolMetrics {
            frame_capacity: 4,
            resident_pages: 2,
            dirty_pages: 1,
            cache_hits: 3,
            cache_misses: 1,
            eviction_stalls: 1,
            flush_stalls: 2,
        };

        metrics.validate().expect("coherent metrics validate");
        assert_eq!(metrics.total_accesses(), 4);
        assert_eq!(metrics.total_stalls(), 3);
        assert_eq!(metrics.hit_ratio(), Some(0.75));
        assert_eq!(metrics.dirty_ratio(), Some(0.5));
        assert!(metrics.has_stalls());
    }

    #[test]
    fn buffer_pool_metrics_track_hits_misses_dirty_ratio_and_flush_stalls() {
        let page_id = PageId::new(10);
        let mut pool = BufferPool::new(
            BufferPoolConfig::new(1, PageSize::KiB16).expect("valid buffer pool config"),
            store_with_pages(&[(page_id, Lsn::new(100))]),
        )
        .expect("valid buffer pool");

        let first_frame =
            BufferPoolManager::pin_page(&mut pool, page_id).expect("first access loads page");
        BufferPoolManager::unpin_page(&mut pool, first_frame, None).expect("release page pin");

        let second_frame =
            BufferPoolManager::pin_page(&mut pool, page_id).expect("resident page hits cache");
        BufferPoolManager::unpin_page(&mut pool, second_frame, None).expect("release page pin");

        {
            let mut guard = pool
                .fetch_page_mut(page_id)
                .expect("resident page can be fetched mutably");
            guard
                .mark_dirty(Lsn::new(120))
                .expect("dirty mark is valid");
        }

        let observer = TestWalDurabilityObserver::with_durable_lsn(110);
        let report = pool
            .flush_all_dirty_with_report(&observer)
            .expect("flush report succeeds");
        assert_eq!(report.flushed, 0);
        assert_eq!(report.blocked_by_wal_durability.len(), 1);

        let metrics = pool.metrics();
        metrics.validate().expect("runtime metrics remain coherent");
        assert_eq!(metrics.cache_hits, 2);
        assert_eq!(metrics.cache_misses, 1);
        assert_eq!(metrics.flush_stalls, 1);
        assert_eq!(metrics.eviction_stalls, 0);
        assert_eq!(metrics.hit_ratio(), Some(2.0 / 3.0));
        assert_eq!(metrics.dirty_ratio(), Some(1.0));
    }

    #[test]
    fn buffer_pool_metrics_track_eviction_stalls() {
        let first_page = PageId::new(20);
        let second_page = PageId::new(21);
        let mut pool = BufferPool::new(
            BufferPoolConfig::new(1, PageSize::KiB16).expect("valid single-frame pool"),
            store_with_pages(&[(first_page, Lsn::new(200)), (second_page, Lsn::new(201))]),
        )
        .expect("valid buffer pool");

        let pinned_frame =
            BufferPoolManager::pin_page(&mut pool, first_page).expect("first page pins");
        let error =
            BufferPoolManager::pin_page(&mut pool, second_page).expect_err("second page stalls");
        assert_eq!(error.kind(), AndromedaErrorKind::Storage);

        let metrics = pool.metrics();
        metrics
            .validate()
            .expect("eviction metrics remain coherent");
        assert_eq!(metrics.cache_hits, 0);
        assert_eq!(metrics.cache_misses, 2);
        assert_eq!(metrics.eviction_stalls, 1);
        assert_eq!(metrics.total_stalls(), 1);

        BufferPoolManager::unpin_page(&mut pool, pinned_frame, None).expect("release pinned page");
    }
}
