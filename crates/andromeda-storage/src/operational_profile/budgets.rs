use crate::{
    HotColdIoThresholds, IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget,
    IoUseClass, PageIoBudget, PageSize, SegmentIoBudget,
};

pub(super) const fn hot_page_budget(
    read_us: u64,
    write_us: u64,
    flush_us: u64,
    throughput_bytes_per_sec: u64,
) -> PageIoBudget {
    PageIoBudget::new(
        PageSize::KiB16,
        IoUseClass::OnlineHotPath,
        IoPathBudget::new(
            IoPathClass::HotPathNvmeSsd,
            IoLatencyBudget::new(read_us, write_us, flush_us),
            IoThroughputBudget::new(throughput_bytes_per_sec, throughput_bytes_per_sec),
        ),
    )
}

pub(super) const fn hot_segment_budget(
    segment_bytes: u64,
    read_us: u64,
    write_us: u64,
    flush_us: u64,
    throughput_bytes_per_sec: u64,
    thresholds: HotColdIoThresholds,
) -> SegmentIoBudget {
    SegmentIoBudget::new(
        segment_bytes,
        IoUseClass::CommitCriticalHotPath,
        IoPathBudget::new(
            IoPathClass::HotPathNvmeSsd,
            IoLatencyBudget::new(read_us, write_us, flush_us),
            IoThroughputBudget::new(throughput_bytes_per_sec, throughput_bytes_per_sec),
        ),
        thresholds,
    )
}

pub(super) const fn cold_segment_budget(
    segment_bytes: u64,
    read_us: u64,
    write_us: u64,
    flush_us: u64,
    throughput_bytes_per_sec: u64,
    thresholds: HotColdIoThresholds,
) -> SegmentIoBudget {
    SegmentIoBudget::new(
        segment_bytes,
        IoUseClass::ColdSegmentPath,
        IoPathBudget::new(
            IoPathClass::ColdPathHdd,
            IoLatencyBudget::new(read_us, write_us, flush_us),
            IoThroughputBudget::new(throughput_bytes_per_sec, throughput_bytes_per_sec),
        ),
        thresholds,
    )
}
