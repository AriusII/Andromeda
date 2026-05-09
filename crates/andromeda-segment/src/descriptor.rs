use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    AllocationId, ExtentId, Lsn, ObjectId, PageId, PageSize, SegmentDurabilityBoundary,
    SegmentHeader, SegmentId, SegmentMutation, SegmentState, SegmentTrailer,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentDescriptor {
    pub segment_id: SegmentId,
    pub object_id: ObjectId,
    pub allocation_id: AllocationId,
    pub first_extent_id: ExtentId,
    pub extent_count: u32,
    pub first_page_id: PageId,
    pub page_count: u32,
    pub page_size: PageSize,
    pub min_page_lsn: Lsn,
    pub max_page_lsn: Lsn,
    pub snapshot_id: Option<u64>,
    pub state: SegmentState,
    pub header: SegmentHeader,
    pub trailer: SegmentTrailer,
}

impl SegmentDescriptor {
    pub fn durability_boundary(&self) -> SegmentDurabilityBoundary {
        SegmentDurabilityBoundary {
            segment_id: self.segment_id,
            object_id: self.object_id.get(),
            allocation_id: self.allocation_id.get(),
            first_extent_id: self.first_extent_id.get(),
            extent_count: self.extent_count,
            first_page_id: self.first_page_id.get(),
            page_count: self.page_count,
            min_page_lsn: self.min_page_lsn,
            max_page_lsn: self.max_page_lsn,
            snapshot_id: self.snapshot_id,
            state: self.state,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.durability_boundary().validate()?;

        self.header.validate()?;
        self.trailer.validate()?;

        if self.header.segment_id != self.segment_id
            || self.header.object_id != self.object_id
            || self.header.allocation_id != self.allocation_id
            || self.header.first_page_id != self.first_page_id
            || self.header.page_count != self.page_count
            || self.header.min_page_lsn != self.min_page_lsn
            || self.header.max_page_lsn != self.max_page_lsn
        {
            return Err(storage_error(
                "segment header does not match segment descriptor",
            ));
        }

        Ok(())
    }

    pub fn validate_mutation(&self, mutation: SegmentMutation) -> AndromedaResult<()> {
        self.validate()?;
        self.durability_boundary().validate_mutation(mutation)
    }
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> SegmentDescriptor {
        let header = SegmentHeader {
            magic: SegmentHeader::MAGIC,
            format_version: SegmentHeader::FORMAT_VERSION_V0,
            segment_id: SegmentId::new(10),
            object_id: ObjectId::new(11),
            allocation_id: AllocationId::new(12),
            first_page_id: PageId::new(1000),
            page_count: 8,
            min_page_lsn: Lsn::new(20),
            max_page_lsn: Lsn::new(30),
            header_crc: 90,
        };
        SegmentDescriptor {
            segment_id: header.segment_id,
            object_id: header.object_id,
            allocation_id: header.allocation_id,
            first_extent_id: ExtentId::new(1),
            extent_count: 1,
            first_page_id: header.first_page_id,
            page_count: header.page_count,
            page_size: PageSize::KiB16,
            min_page_lsn: header.min_page_lsn,
            max_page_lsn: header.max_page_lsn,
            snapshot_id: Some(77),
            state: SegmentState::PublishedCold,
            header,
            trailer: SegmentTrailer {
                segment_payload_crc64: 1,
                segment_hash: [2; 32],
                trailer_crc: 3,
            },
        }
    }

    #[test]
    fn segment_descriptor_validates_header_and_trailer() {
        assert!(descriptor().validate().is_ok());
    }

    #[test]
    fn segment_descriptor_rejects_header_mismatch() {
        let mut descriptor = descriptor();
        descriptor.header.page_count += 1;

        assert_eq!(
            descriptor.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn segment_descriptor_rejects_update_or_split_after_publication() {
        let descriptor = descriptor();

        assert_eq!(
            descriptor
                .validate_mutation(SegmentMutation::UpdatePageInPlace)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
        assert_eq!(
            descriptor
                .validate_mutation(SegmentMutation::SplitSegment)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }
}
