use crate::{Lsn, PageId, PageSize, SegmentId, SegmentState};

use super::super::{
    error::{SegmentIndexError, SegmentIndexResult},
    types::SegmentIndexEntryV0,
};
use super::{
    PAGE_SIZE_POLICY_16K, PAGE_SIZE_POLICY_32K, PAGE_SIZE_POLICY_MIXED, PAGE_SIZE_TAG_16K,
    PAGE_SIZE_TAG_32K, SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES,
    SEGMENT_INDEX_V0_FLAG_PUBLISHED_COLD_ONLY, SEGMENT_STATE_TAG_BUILDING_HOT_SNAPSHOT,
    SEGMENT_STATE_TAG_PUBLISHED_COLD, SEGMENT_STATE_TAG_SEALED,
};

pub(super) fn summarize_entries(
    entries: &[SegmentIndexEntryV0],
) -> SegmentIndexResult<EntrySummary> {
    let first = entries.first().ok_or(SegmentIndexError::InvalidHeader {
        reason: "entry count must not be zero",
    })?;
    let last = entries.last().ok_or(SegmentIndexError::InvalidHeader {
        reason: "entry count must not be zero",
    })?;

    let mut min_page_lsn = first.min_page_lsn;
    let mut max_page_lsn = first.max_page_lsn;
    let mut total_page_count = 0_u64;
    for entry in entries {
        min_page_lsn = min_page_lsn.min(entry.min_page_lsn);
        max_page_lsn = max_page_lsn.max(entry.max_page_lsn);
        total_page_count = total_page_count
            .checked_add(u64::from(entry.page_count))
            .ok_or(SegmentIndexError::LengthOverflow {
                field: "total_page_count",
            })?;
    }

    Ok(EntrySummary {
        first_segment_id: first.segment_id,
        last_segment_id: last.segment_id,
        first_page_id: first.first_page_id,
        last_page_id: last.last_page_id()?,
        min_page_lsn,
        max_page_lsn,
        total_page_count,
    })
}

pub(super) fn computed_flags(entries: &[SegmentIndexEntryV0]) -> u32 {
    let mut flags = SEGMENT_INDEX_V0_FLAG_PUBLISHED_COLD_ONLY;
    if entries.windows(2).all(|window| {
        window[0]
            .last_page_id()
            .map(|last| window[1].first_page_id.get() == last.get() + 1)
            .unwrap_or(false)
    }) {
        flags |= SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES;
    }
    flags
}

pub(super) fn page_size_policy(entries: &[SegmentIndexEntryV0]) -> SegmentIndexResult<u32> {
    let first = entries.first().ok_or(SegmentIndexError::InvalidHeader {
        reason: "entry count must not be zero",
    })?;
    if entries
        .iter()
        .all(|entry| entry.page_size == first.page_size)
    {
        return Ok(match first.page_size {
            PageSize::KiB16 => PAGE_SIZE_POLICY_16K,
            PageSize::KiB32 => PAGE_SIZE_POLICY_32K,
        });
    }
    Ok(PAGE_SIZE_POLICY_MIXED)
}

pub(super) fn validate_page_size_policy(tag: u32) -> SegmentIndexResult<()> {
    match tag {
        PAGE_SIZE_POLICY_MIXED | PAGE_SIZE_POLICY_16K | PAGE_SIZE_POLICY_32K => Ok(()),
        _ => Err(SegmentIndexError::InvalidHeader {
            reason: "unknown page size policy tag",
        }),
    }
}

pub(super) fn page_size_tag(page_size: PageSize) -> u16 {
    match page_size {
        PageSize::KiB16 => PAGE_SIZE_TAG_16K,
        PageSize::KiB32 => PAGE_SIZE_TAG_32K,
    }
}

pub(super) fn decode_page_size_tag(tag: u16, index: usize) -> SegmentIndexResult<PageSize> {
    match tag {
        PAGE_SIZE_TAG_16K => Ok(PageSize::KiB16),
        PAGE_SIZE_TAG_32K => Ok(PageSize::KiB32),
        _ => Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "unknown page size tag",
        }),
    }
}

pub(super) fn segment_state_tag(state: SegmentState) -> u16 {
    match state {
        SegmentState::BuildingHotSnapshot => SEGMENT_STATE_TAG_BUILDING_HOT_SNAPSHOT,
        SegmentState::Sealed => SEGMENT_STATE_TAG_SEALED,
        SegmentState::PublishedCold => SEGMENT_STATE_TAG_PUBLISHED_COLD,
    }
}

pub(super) fn decode_segment_state_tag(tag: u16, index: usize) -> SegmentIndexResult<SegmentState> {
    match tag {
        SEGMENT_STATE_TAG_BUILDING_HOT_SNAPSHOT => Ok(SegmentState::BuildingHotSnapshot),
        SEGMENT_STATE_TAG_SEALED => Ok(SegmentState::Sealed),
        SEGMENT_STATE_TAG_PUBLISHED_COLD => Ok(SegmentState::PublishedCold),
        _ => Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "unknown segment state tag",
        }),
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct EntrySummary {
    pub(super) first_segment_id: SegmentId,
    pub(super) last_segment_id: SegmentId,
    pub(super) first_page_id: PageId,
    pub(super) last_page_id: PageId,
    pub(super) min_page_lsn: Lsn,
    pub(super) max_page_lsn: Lsn,
    pub(super) total_page_count: u64,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TrailerHashMode {
    Stored,
    AcyclicFileHashInput,
}
