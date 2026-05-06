use std::collections::BTreeMap;

use andromeda_core::AndromedaResult;

use crate::{PageId, SegmentId};

use super::{
    ColdExtentReclaimEvidence, ExtentDescriptor, ExtentFreeRange, ExtentId, ExtentManagerReplayRecord,
    ExtentState, error::storage_error,
};

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

    pub fn apply_replay_record(&mut self, record: ExtentManagerReplayRecord) -> AndromedaResult<()> {
        match record {
            ExtentManagerReplayRecord::AllocateHot(descriptor) => self.allocate_hot(descriptor),
            ExtentManagerReplayRecord::Seal { extent_id } => self.seal_extent(extent_id),
            ExtentManagerReplayRecord::PublishCold {
                extent_id,
                segment_id,
            } => self.publish_cold_extent(extent_id, segment_id),
            ExtentManagerReplayRecord::FreeSealed { extent_id } => self.free_sealed_extent(extent_id),
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
