use std::collections::BTreeMap;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    AllocationId, Lsn, ObjectId, PageId, PageSize, SegmentDescriptor, SegmentId, SegmentState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtentId(u64);

impl ExtentId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtentState {
    AllocatingHot,
    Sealed,
    PublishedCold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtentDescriptor {
    pub extent_id: ExtentId,
    pub object_id: ObjectId,
    pub allocation_id: AllocationId,
    pub first_page_id: PageId,
    pub page_count: u32,
    pub page_size: PageSize,
    pub state: ExtentState,
    pub segment_id: Option<SegmentId>,
    /// File offset where this extent begins on disk (hot-store).
    /// Set during allocation; read from manifest during recovery.
    pub file_offset: u64,
    /// Whether pages in this extent have been actually written to disk.
    /// Used for contiguity checks and allocation tracking.
    pub allocated_on_disk: bool,
}

impl ExtentDescriptor {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.extent_id.is_zero() {
            return Err(storage_error("extent id must not be zero"));
        }
        if self.object_id.is_zero() || self.allocation_id.is_zero() {
            return Err(storage_error(
                "extent object/allocation identity must not be zero",
            ));
        }
        if self.first_page_id.is_zero() {
            return Err(storage_error("extent first page id must not be zero"));
        }
        if self.page_count == 0 {
            return Err(storage_error("extent page count must not be zero"));
        }
        if self
            .first_page_id
            .get()
            .checked_add(u64::from(self.page_count - 1))
            .is_none()
        {
            return Err(storage_error("extent page range overflows u64"));
        }
        if matches!(self.segment_id, Some(segment_id) if segment_id.is_zero()) {
            return Err(storage_error("extent segment id must not be zero"));
        }
        if self.state == ExtentState::PublishedCold && self.segment_id.is_none() {
            return Err(storage_error(
                "published cold extent must reference a segment",
            ));
        }
        Ok(())
    }

    pub fn last_page_id(&self) -> AndromedaResult<PageId> {
        self.validate()?;
        Ok(PageId::new(
            self.first_page_id.get() + u64::from(self.page_count - 1),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtentFreeRange {
    pub first_page_id: PageId,
    pub page_count: u32,
    pub page_size: PageSize,
    pub recyclable_after_lsn: Option<Lsn>,
}

impl ExtentFreeRange {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.first_page_id.is_zero() || self.page_count == 0 {
            return Err(storage_error("free extent range must not be empty"));
        }
        if self
            .first_page_id
            .get()
            .checked_add(u64::from(self.page_count - 1))
            .is_none()
        {
            return Err(storage_error("free extent range overflows u64"));
        }
        if matches!(self.recyclable_after_lsn, Some(lsn) if lsn.is_zero()) {
            return Err(storage_error(
                "free extent recyclable-after LSN must not be zero",
            ));
        }
        Ok(())
    }

    pub fn last_page_id(&self) -> AndromedaResult<PageId> {
        self.validate()?;
        Ok(PageId::new(
            self.first_page_id.get() + u64::from(self.page_count - 1),
        ))
    }

    fn contains_descriptor(&self, descriptor: &ExtentDescriptor) -> AndromedaResult<bool> {
        self.validate()?;
        descriptor.validate()?;
        let range_end = self.last_page_id()?.get();
        let descriptor_end = descriptor.last_page_id()?.get();
        Ok(self.page_size == descriptor.page_size
            && self.first_page_id.get() <= descriptor.first_page_id.get()
            && descriptor_end <= range_end)
    }

    fn overlaps_descriptor(&self, descriptor: &ExtentDescriptor) -> AndromedaResult<bool> {
        self.validate()?;
        descriptor.validate()?;
        let start = self.first_page_id.get();
        let end = self.last_page_id()?.get();
        let other_start = descriptor.first_page_id.get();
        let other_end = descriptor.last_page_id()?.get();
        Ok(start <= other_end && other_start <= end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColdExtentReclaimEvidence {
    pub retired_manifest_version: u64,
    pub reclaim_lsn: Lsn,
    pub catalog_epoch: u64,
}

impl ColdExtentReclaimEvidence {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.retired_manifest_version == 0 || self.catalog_epoch == 0 {
            return Err(storage_error(
                "cold extent reclaim evidence must carry nonzero manifest and catalog anchors",
            ));
        }
        if self.reclaim_lsn.is_zero() {
            return Err(storage_error("cold extent reclaim LSN must not be zero"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtentManagerReplayRecord {
    AllocateHot(ExtentDescriptor),
    Seal {
        extent_id: ExtentId,
    },
    PublishCold {
        extent_id: ExtentId,
        segment_id: SegmentId,
    },
    FreeSealed {
        extent_id: ExtentId,
    },
    ReclaimPublishedCold {
        extent_id: ExtentId,
        evidence: ColdExtentReclaimEvidence,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtentManager {
    extents: BTreeMap<ExtentId, ExtentDescriptor>,
    free_list: Vec<ExtentFreeRange>,
}

impl ExtentManager {
    pub fn new(free_list: Vec<ExtentFreeRange>) -> AndromedaResult<Self> {
        let manager = Self {
            extents: BTreeMap::new(),
            free_list,
        };
        manager.validate_free_list()?;
        Ok(manager)
    }

    pub fn from_descriptors(descriptors: Vec<ExtentDescriptor>) -> AndromedaResult<Self> {
        let mut manager = Self::new(Vec::new())?;
        for descriptor in descriptors {
            manager.insert_replayed_descriptor(descriptor)?;
        }
        Ok(manager)
    }

    pub fn extents(&self) -> impl Iterator<Item = &ExtentDescriptor> {
        self.extents.values()
    }

    pub fn free_ranges(&self) -> &[ExtentFreeRange] {
        &self.free_list
    }

    pub fn descriptor(&self, extent_id: ExtentId) -> Option<&ExtentDescriptor> {
        self.extents.get(&extent_id)
    }

    pub fn allocate_hot(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()> {
        descriptor.validate()?;
        if descriptor.state != ExtentState::AllocatingHot {
            return Err(storage_error(
                "extent allocation must enter through AllocatingHot state",
            ));
        }
        if descriptor.segment_id.is_some() {
            return Err(storage_error(
                "hot allocation must not pre-bind an extent to a cold segment",
            ));
        }
        if self.extents.contains_key(&descriptor.extent_id) {
            return Err(storage_error("extent id is already allocated"));
        }
        self.reject_allocated_overlap(&descriptor)?;
        self.consume_free_range(&descriptor)?;
        self.extents.insert(descriptor.extent_id, descriptor);
        Ok(())
    }

    pub fn seal_extent(&mut self, extent_id: ExtentId) -> AndromedaResult<()> {
        let descriptor = self
            .extents
            .get_mut(&extent_id)
            .ok_or_else(|| storage_error("extent id is not allocated"))?;
        descriptor.validate()?;
        if descriptor.state != ExtentState::AllocatingHot {
            return Err(storage_error(
                "only an allocating hot extent can transition to sealed",
            ));
        }
        descriptor.state = ExtentState::Sealed;
        Ok(())
    }

    pub fn publish_cold_extent(
        &mut self,
        extent_id: ExtentId,
        segment_id: SegmentId,
    ) -> AndromedaResult<()> {
        if segment_id.is_zero() {
            return Err(storage_error(
                "published cold extent segment id must not be zero",
            ));
        }
        let descriptor = self
            .extents
            .get_mut(&extent_id)
            .ok_or_else(|| storage_error("extent id is not allocated"))?;
        descriptor.validate()?;
        if descriptor.state != ExtentState::Sealed {
            return Err(storage_error(
                "cold publication requires a sealed extent and never mutates published cold",
            ));
        }
        descriptor.state = ExtentState::PublishedCold;
        descriptor.segment_id = Some(segment_id);
        descriptor.validate()
    }

    pub fn free_sealed_extent(&mut self, extent_id: ExtentId) -> AndromedaResult<()> {
        let descriptor = *self
            .extents
            .get(&extent_id)
            .ok_or_else(|| storage_error("extent id is not allocated"))?;
        descriptor.validate()?;
        if descriptor.state != ExtentState::Sealed {
            return Err(storage_error(
                "only sealed, unpublished extents can return directly to the free list",
            ));
        }
        self.extents.remove(&extent_id);
        self.add_free_range(ExtentFreeRange {
            first_page_id: descriptor.first_page_id,
            page_count: descriptor.page_count,
            page_size: descriptor.page_size,
            recyclable_after_lsn: None,
        })
    }

    pub fn reclaim_published_cold_extent(
        &mut self,
        extent_id: ExtentId,
        evidence: ColdExtentReclaimEvidence,
    ) -> AndromedaResult<()> {
        evidence.validate()?;
        let descriptor = *self
            .extents
            .get(&extent_id)
            .ok_or_else(|| storage_error("extent id is not allocated"))?;
        descriptor.validate()?;
        if descriptor.state != ExtentState::PublishedCold {
            return Err(storage_error(
                "cold reclaim evidence applies only to published cold extents",
            ));
        }
        self.extents.remove(&extent_id);
        self.add_free_range(ExtentFreeRange {
            first_page_id: descriptor.first_page_id,
            page_count: descriptor.page_count,
            page_size: descriptor.page_size,
            recyclable_after_lsn: Some(evidence.reclaim_lsn),
        })
    }

    pub fn apply_replay_record(
        &mut self,
        record: ExtentManagerReplayRecord,
    ) -> AndromedaResult<()> {
        match record {
            ExtentManagerReplayRecord::AllocateHot(descriptor) => self.allocate_hot(descriptor),
            ExtentManagerReplayRecord::Seal { extent_id } => self.seal_extent(extent_id),
            ExtentManagerReplayRecord::PublishCold {
                extent_id,
                segment_id,
            } => self.publish_cold_extent(extent_id, segment_id),
            ExtentManagerReplayRecord::FreeSealed { extent_id } => {
                self.free_sealed_extent(extent_id)
            }
            ExtentManagerReplayRecord::ReclaimPublishedCold {
                extent_id,
                evidence,
            } => self.reclaim_published_cold_extent(extent_id, evidence),
        }
    }

    fn insert_replayed_descriptor(&mut self, descriptor: ExtentDescriptor) -> AndromedaResult<()> {
        descriptor.validate()?;
        if self.extents.contains_key(&descriptor.extent_id) {
            return Err(storage_error("extent id is duplicated during replay"));
        }
        self.reject_allocated_overlap(&descriptor)?;
        self.extents.insert(descriptor.extent_id, descriptor);
        Ok(())
    }

    fn reject_allocated_overlap(&self, descriptor: &ExtentDescriptor) -> AndromedaResult<()> {
        for allocated in self.extents.values() {
            let start = allocated.first_page_id.get();
            let end = allocated.last_page_id()?.get();
            let other_start = descriptor.first_page_id.get();
            let other_end = descriptor.last_page_id()?.get();
            if start <= other_end && other_start <= end {
                return Err(storage_error(
                    "extent allocation overlaps an allocated extent",
                ));
            }
        }
        Ok(())
    }

    fn consume_free_range(&mut self, descriptor: &ExtentDescriptor) -> AndromedaResult<()> {
        let Some(index) = self
            .free_list
            .iter()
            .position(|range| range.contains_descriptor(descriptor).unwrap_or(false))
        else {
            return Err(storage_error(
                "extent allocation must be backed by a contiguous free range",
            ));
        };

        let range = self.free_list.remove(index);
        if range.recyclable_after_lsn.is_some() {
            self.free_list.push(range);
            return Err(storage_error(
                "reclaimable cold extent range requires recovery/catalog floor proof before reuse",
            ));
        }
        let descriptor_start = descriptor.first_page_id.get();
        let descriptor_end = descriptor.last_page_id()?.get();
        let range_start = range.first_page_id.get();
        let range_end = range.last_page_id()?.get();

        if range_start < descriptor_start {
            self.free_list.push(ExtentFreeRange {
                first_page_id: range.first_page_id,
                page_count: u32::try_from(descriptor_start - range_start)
                    .map_err(|_| storage_error("left free extent fragment exceeds u32 pages"))?,
                page_size: range.page_size,
                recyclable_after_lsn: range.recyclable_after_lsn,
            });
        }
        if descriptor_end < range_end {
            self.free_list.push(ExtentFreeRange {
                first_page_id: PageId::new(descriptor_end + 1),
                page_count: u32::try_from(range_end - descriptor_end)
                    .map_err(|_| storage_error("right free extent fragment exceeds u32 pages"))?,
                page_size: range.page_size,
                recyclable_after_lsn: range.recyclable_after_lsn,
            });
        }
        self.validate_free_list()
    }

    fn add_free_range(&mut self, range: ExtentFreeRange) -> AndromedaResult<()> {
        range.validate()?;
        for allocated in self.extents.values() {
            if range.overlaps_descriptor(allocated)? {
                return Err(storage_error("free range overlaps an allocated extent"));
            }
        }
        self.free_list.push(range);
        self.validate_free_list()
    }

    fn validate_free_list(&self) -> AndromedaResult<()> {
        for (index, range) in self.free_list.iter().enumerate() {
            range.validate()?;
            for previous in &self.free_list[..index] {
                let previous_start = previous.first_page_id.get();
                let previous_end = previous.last_page_id()?.get();
                let range_start = range.first_page_id.get();
                let range_end = range.last_page_id()?.get();
                if previous_start <= range_end && range_start <= previous_end {
                    return Err(storage_error("free extent ranges must not overlap"));
                }
            }
        }
        Ok(())
    }
}

pub fn validate_segment_extent_contiguity(
    segment: &SegmentDescriptor,
    extents: &[ExtentDescriptor],
) -> AndromedaResult<()> {
    segment.validate()?;
    if extents.len()
        != usize::try_from(segment.extent_count)
            .map_err(|_| storage_error("segment extent count does not fit usize"))?
    {
        return Err(storage_error(
            "segment extent list must match segment extent count",
        ));
    }

    let mut expected_extent_id = segment.first_extent_id.get();
    let mut expected_first_page = segment.first_page_id.get();
    let segment_last_page = segment
        .first_page_id
        .get()
        .checked_add(u64::from(segment.page_count - 1))
        .ok_or_else(|| storage_error("segment page range overflows u64"))?;

    for extent in extents {
        extent.validate()?;
        if extent.extent_id.get() != expected_extent_id {
            return Err(storage_error(
                "segment extents must use contiguous extent identifiers",
            ));
        }
        if extent.object_id != segment.object_id
            || extent.allocation_id != segment.allocation_id
            || extent.page_size != segment.page_size
        {
            return Err(storage_error(
                "segment extent identity and page size must match segment descriptor",
            ));
        }
        if extent.first_page_id.get() != expected_first_page {
            return Err(storage_error(
                "segment extents must cover a physically contiguous page range",
            ));
        }
        if extent.last_page_id()?.get() > segment_last_page {
            return Err(storage_error("segment extent page range exceeds segment"));
        }
        if segment.state == SegmentState::PublishedCold {
            if extent.state != ExtentState::PublishedCold
                || extent.segment_id != Some(segment.segment_id)
            {
                return Err(storage_error(
                    "published cold segment requires all extents to be published and segment-bound",
                ));
            }
        }
        expected_extent_id = expected_extent_id
            .checked_add(1)
            .ok_or_else(|| storage_error("segment extent id range overflows u64"))?;
        expected_first_page = extent
            .last_page_id()?
            .get()
            .checked_add(1)
            .ok_or_else(|| storage_error("segment page range overflows u64"))?;
    }

    let expected_after_segment = segment_last_page
        .checked_add(1)
        .ok_or_else(|| storage_error("segment page range overflows u64"))?;
    if expected_first_page != expected_after_segment {
        return Err(storage_error(
            "segment extents must exactly cover the segment page range",
        ));
    }
    Ok(())
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extent() -> ExtentDescriptor {
        ExtentDescriptor {
            extent_id: ExtentId::new(1),
            object_id: ObjectId::new(2),
            allocation_id: AllocationId::new(3),
            first_page_id: PageId::new(100),
            page_count: 16,
            page_size: PageSize::KiB16,
            state: ExtentState::PublishedCold,
            segment_id: Some(SegmentId::new(4)),
            file_offset: 0,
            allocated_on_disk: false,
        }
    }

    fn segment() -> SegmentDescriptor {
        let header = crate::SegmentHeader {
            magic: crate::SegmentHeader::MAGIC,
            format_version: crate::SegmentHeader::FORMAT_VERSION_V0,
            segment_id: SegmentId::new(4),
            object_id: ObjectId::new(2),
            allocation_id: AllocationId::new(3),
            first_page_id: PageId::new(100),
            page_count: 16,
            min_page_lsn: Lsn::new(10),
            max_page_lsn: Lsn::new(20),
            header_crc: 30,
        };
        SegmentDescriptor {
            segment_id: header.segment_id,
            object_id: header.object_id,
            allocation_id: header.allocation_id,
            first_extent_id: ExtentId::new(1),
            extent_count: 2,
            first_page_id: header.first_page_id,
            page_count: header.page_count,
            page_size: PageSize::KiB16,
            min_page_lsn: header.min_page_lsn,
            max_page_lsn: header.max_page_lsn,
            snapshot_id: Some(40),
            state: SegmentState::PublishedCold,
            header,
            trailer: crate::SegmentTrailer {
                segment_payload_crc64: 50,
                segment_hash: [60; 32],
                trailer_crc: 70,
            },
        }
    }

    #[test]
    fn extent_contract_validates_page_range() {
        let descriptor = extent();
        assert!(descriptor.validate().is_ok());
        assert_eq!(descriptor.last_page_id().unwrap(), PageId::new(115));
    }

    #[test]
    fn extent_contract_rejects_published_extent_without_segment() {
        let mut descriptor = extent();
        descriptor.segment_id = None;

        assert_eq!(
            descriptor.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn extent_manager_allocates_from_free_range_and_replays_lifecycle() {
        let free = ExtentFreeRange {
            first_page_id: PageId::new(100),
            page_count: 32,
            page_size: PageSize::KiB16,
            recyclable_after_lsn: None,
        };
        let mut manager = ExtentManager::new(vec![free]).unwrap();
        let mut descriptor = extent();
        descriptor.state = ExtentState::AllocatingHot;
        descriptor.segment_id = None;

        manager
            .apply_replay_record(ExtentManagerReplayRecord::AllocateHot(descriptor))
            .unwrap();
        manager
            .apply_replay_record(ExtentManagerReplayRecord::Seal {
                extent_id: descriptor.extent_id,
            })
            .unwrap();
        manager
            .apply_replay_record(ExtentManagerReplayRecord::PublishCold {
                extent_id: descriptor.extent_id,
                segment_id: SegmentId::new(4),
            })
            .unwrap();

        let published = manager.descriptor(descriptor.extent_id).unwrap();
        assert_eq!(published.state, ExtentState::PublishedCold);
        assert_eq!(published.segment_id, Some(SegmentId::new(4)));
        assert_eq!(manager.free_ranges().len(), 1);
        assert_eq!(manager.free_ranges()[0].first_page_id, PageId::new(116));
    }

    #[test]
    fn extent_manager_rejects_direct_free_after_cold_publication() {
        let mut manager = ExtentManager::from_descriptors(vec![extent()]).unwrap();

        assert_eq!(
            manager
                .free_sealed_extent(ExtentId::new(1))
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
        assert!(manager.descriptor(ExtentId::new(1)).is_some());
    }

    #[test]
    fn extent_manager_reclaims_published_cold_only_with_manifest_evidence() {
        let mut manager = ExtentManager::from_descriptors(vec![extent()]).unwrap();

        manager
            .reclaim_published_cold_extent(
                ExtentId::new(1),
                ColdExtentReclaimEvidence {
                    retired_manifest_version: 2,
                    reclaim_lsn: Lsn::new(30),
                    catalog_epoch: 3,
                },
            )
            .unwrap();

        assert!(manager.descriptor(ExtentId::new(1)).is_none());
        assert_eq!(
            manager.free_ranges()[0].recyclable_after_lsn,
            Some(Lsn::new(30))
        );
    }

    #[test]
    fn segment_extent_contiguity_requires_exact_physical_coverage() {
        let segment = segment();
        let mut first = extent();
        first.page_count = 8;
        let mut second = first;
        second.extent_id = ExtentId::new(2);
        second.first_page_id = PageId::new(108);
        second.page_count = 8;

        assert!(validate_segment_extent_contiguity(&segment, &[first, second]).is_ok());

        second.first_page_id = PageId::new(109);
        assert_eq!(
            validate_segment_extent_contiguity(&segment, &[first, second])
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }
}
