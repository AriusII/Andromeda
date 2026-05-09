//! Runtime-free cold publication manifest boundaries.
//!
//! These types validate immutable publication state over segment descriptors.
//! Concrete archive IO, storage placement, and publication scheduling remain in
//! their owner crates.

use andromeda_error::AndromedaResult;
use andromeda_segment::{SegmentDescriptor, SegmentMutation, SegmentState};

use crate::manifest_error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedColdSegment {
    pub descriptor: SegmentDescriptor,
}

impl PublishedColdSegment {
    pub fn new(descriptor: SegmentDescriptor) -> AndromedaResult<Self> {
        descriptor.validate()?;
        if descriptor.state != SegmentState::PublishedCold {
            return Err(manifest_error(
                "PublishedColdSegment requires a published cold descriptor",
            ));
        }
        Ok(Self { descriptor })
    }

    pub fn validate_immutable(&self) -> AndromedaResult<()> {
        self.descriptor.validate()?;
        if self.descriptor.state != SegmentState::PublishedCold {
            return Err(manifest_error("cold segment is not in published state"));
        }
        Ok(())
    }

    pub fn reject_mutation(&self, mutation: SegmentMutation) -> AndromedaResult<()> {
        let _ = mutation;
        Err(manifest_error(
            "PublishedColdSegment rejects all mutations; ColdStore is immutable after publication",
        ))
    }
}

#[cfg(test)]
mod tests {
    use andromeda_error::AndromedaErrorKind;
    use andromeda_segment::{
        AllocationId, ExtentId, Lsn, ObjectId, PageId, PageSize, SegmentDescriptor, SegmentHeader,
        SegmentId, SegmentMutation, SegmentState, SegmentTrailer,
    };

    use super::*;

    fn descriptor(state: SegmentState) -> SegmentDescriptor {
        let header = SegmentHeader {
            magic: SegmentHeader::MAGIC,
            format_version: SegmentHeader::FORMAT_VERSION_V0,
            segment_id: SegmentId::new(21),
            object_id: ObjectId::new(22),
            allocation_id: AllocationId::new(23),
            first_page_id: PageId::new(24),
            page_count: 2,
            min_page_lsn: Lsn::new(25),
            max_page_lsn: Lsn::new(26),
            header_crc: 27,
        };
        SegmentDescriptor {
            segment_id: header.segment_id,
            object_id: header.object_id,
            allocation_id: header.allocation_id,
            first_extent_id: ExtentId::new(28),
            extent_count: 1,
            first_page_id: header.first_page_id,
            page_count: header.page_count,
            page_size: PageSize::KiB16,
            min_page_lsn: header.min_page_lsn,
            max_page_lsn: header.max_page_lsn,
            snapshot_id: if state == SegmentState::PublishedCold {
                Some(29)
            } else {
                None
            },
            state,
            header,
            trailer: SegmentTrailer {
                segment_payload_crc64: 30,
                segment_hash: [31; 32],
                trailer_crc: 32,
            },
        }
    }

    #[test]
    fn cold_store_requires_published_segment() {
        assert!(PublishedColdSegment::new(descriptor(SegmentState::PublishedCold)).is_ok());
        assert_eq!(
            PublishedColdSegment::new(descriptor(SegmentState::Sealed))
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn published_cold_segment_rejects_mutation() {
        let segment = PublishedColdSegment::new(descriptor(SegmentState::PublishedCold)).unwrap();

        assert_eq!(
            segment
                .reject_mutation(SegmentMutation::AppendExtent)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }
}
