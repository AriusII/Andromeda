#![forbid(unsafe_code)]
#![doc = r#"
Boundary crate for Andromeda durable storage segments.

This crate owns segment durability boundary contracts that are independent of
storage page, extent, and manifest implementations. Storage keeps the current
segment descriptor facade during the migration and delegates boundary
validation here.

C5 invariants:

- Segment publication must be backed by durable WAL and manifest evidence.
- Segment metadata bytes must be versioned and explicitly encoded.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
mod descriptor;
pub mod extent;
pub mod segment_index;

pub use andromeda_storage_page::{AllocationId, ExtentId, ObjectId, PageId, PageSize};
pub use andromeda_wal::Lsn;
pub use descriptor::SegmentDescriptor;
pub use extent::{
    ColdExtentReclaimEvidence, ExtentDescriptor, ExtentFreeRange, ExtentManager,
    ExtentManagerReplayRecord, ExtentState, validate_segment_extent_contiguity,
};

/// Durable segment identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SegmentId(u64);

impl SegmentId {
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

/// Segment lifecycle state used by manifest, segment-index, and ColdStore
/// boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentState {
    BuildingHotSnapshot,
    Sealed,
    PublishedCold,
}

/// Mutation class evaluated against segment immutability boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentMutation {
    AppendExtent,
    UpdatePageInPlace,
    SplitSegment,
}

/// Durable segment header carried with page-backed cold snapshots and sealed
/// segment publications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentHeader {
    pub magic: u32,
    pub format_version: u16,
    pub segment_id: SegmentId,
    pub object_id: ObjectId,
    pub allocation_id: AllocationId,
    pub first_page_id: PageId,
    pub page_count: u32,
    pub min_page_lsn: Lsn,
    pub max_page_lsn: Lsn,
    pub header_crc: u32,
}

impl SegmentHeader {
    pub const MAGIC: u32 = 0x414E4453;
    pub const FORMAT_VERSION_V0: u16 = 1;

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.magic != Self::MAGIC {
            return Err(segment_error("segment header magic mismatch"));
        }
        if self.format_version != Self::FORMAT_VERSION_V0 {
            return Err(segment_error("unsupported segment header format version"));
        }
        if self.segment_id.is_zero() || self.object_id.is_zero() || self.allocation_id.is_zero() {
            return Err(segment_error(
                "segment header identity fields must not be zero",
            ));
        }
        if self.first_page_id.is_zero() {
            return Err(segment_error("segment first page id must not be zero"));
        }
        if self.page_count == 0 {
            return Err(segment_error("segment page count must not be zero"));
        }
        if self.min_page_lsn.is_zero() || self.max_page_lsn.is_zero() {
            return Err(segment_error("segment LSN bounds must not be zero"));
        }
        if self.max_page_lsn < self.min_page_lsn {
            return Err(segment_error("segment max page LSN precedes min page LSN"));
        }
        if self.header_crc == 0 {
            return Err(segment_error("segment header CRC must not be zero"));
        }
        Ok(())
    }
}

/// Durable trailer integrity fields for sealed segment bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentTrailer {
    pub segment_payload_crc64: u64,
    pub segment_hash: [u8; 32],
    pub trailer_crc: u32,
}

impl SegmentTrailer {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.segment_payload_crc64 == 0 {
            return Err(segment_error("segment payload CRC must not be zero"));
        }
        if self.segment_hash == [0; 32] {
            return Err(segment_error("segment hash must not be zero"));
        }
        if self.trailer_crc == 0 {
            return Err(segment_error("segment trailer CRC must not be zero"));
        }
        Ok(())
    }
}

/// Implementation-neutral segment durability fields.
///
/// This boundary intentionally stores page, object, allocation, and extent
/// identities as numeric durable identifiers so segment ownership does not
/// depend on storage's in-memory page and extent modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentDurabilityBoundary {
    pub segment_id: SegmentId,
    pub object_id: u64,
    pub allocation_id: u64,
    pub first_extent_id: u64,
    pub extent_count: u32,
    pub first_page_id: u64,
    pub page_count: u32,
    pub min_page_lsn: Lsn,
    pub max_page_lsn: Lsn,
    pub snapshot_id: Option<u64>,
    pub state: SegmentState,
}

impl SegmentDurabilityBoundary {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.segment_id.is_zero() || self.object_id == 0 || self.allocation_id == 0 {
            return Err(segment_error(
                "segment descriptor identity fields must not be zero",
            ));
        }
        if self.first_extent_id == 0 {
            return Err(segment_error("segment first extent id must not be zero"));
        }
        if self.extent_count == 0 {
            return Err(segment_error("segment extent count must not be zero"));
        }
        if self.first_page_id == 0 || self.page_count == 0 {
            return Err(segment_error("segment page range must not be empty"));
        }
        if self
            .first_page_id
            .checked_add(u64::from(self.page_count - 1))
            .is_none()
        {
            return Err(segment_error("segment page range overflows u64"));
        }
        if self.min_page_lsn.is_zero() || self.max_page_lsn.is_zero() {
            return Err(segment_error("segment page LSN bounds must not be zero"));
        }
        if self.max_page_lsn < self.min_page_lsn {
            return Err(segment_error("segment max page LSN precedes min page LSN"));
        }
        if matches!(self.snapshot_id, Some(0)) {
            return Err(segment_error("segment snapshot id must not be zero"));
        }
        if self.state == SegmentState::PublishedCold && self.snapshot_id.is_none() {
            return Err(segment_error(
                "published cold segment must reference a snapshot",
            ));
        }

        Ok(())
    }

    pub fn validate_mutation(self, mutation: SegmentMutation) -> AndromedaResult<()> {
        self.validate()?;
        if self.state == SegmentState::PublishedCold {
            return Err(segment_error(format!(
                "published cold segment rejects {mutation:?}; ColdStore is immutable after publication"
            )));
        }
        if matches!(
            mutation,
            SegmentMutation::UpdatePageInPlace | SegmentMutation::SplitSegment
        ) && self.state == SegmentState::Sealed
        {
            return Err(segment_error(
                "sealed segment rejects update-in-place and split mutations",
            ));
        }
        Ok(())
    }
}

fn segment_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boundary(state: SegmentState) -> SegmentDurabilityBoundary {
        SegmentDurabilityBoundary {
            segment_id: SegmentId::new(10),
            object_id: 11,
            allocation_id: 12,
            first_extent_id: 13,
            extent_count: 2,
            first_page_id: 100,
            page_count: 8,
            min_page_lsn: Lsn::new(20),
            max_page_lsn: Lsn::new(30),
            snapshot_id: if state == SegmentState::PublishedCold {
                Some(40)
            } else {
                None
            },
            state,
        }
    }

    #[test]
    fn segment_boundary_validates_durable_ranges() {
        assert!(boundary(SegmentState::PublishedCold).validate().is_ok());

        let mut invalid = boundary(SegmentState::PublishedCold);
        invalid.max_page_lsn = Lsn::new(19);
        assert_eq!(
            invalid.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn segment_boundary_rejects_unsafe_published_mutations() {
        assert!(
            boundary(SegmentState::PublishedCold)
                .validate_mutation(SegmentMutation::AppendExtent)
                .is_err()
        );
        assert!(
            boundary(SegmentState::Sealed)
                .validate_mutation(SegmentMutation::UpdatePageInPlace)
                .is_err()
        );
    }

    #[test]
    fn segment_header_and_trailer_validate_integrity_fields() {
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
        let trailer = SegmentTrailer {
            segment_payload_crc64: 1,
            segment_hash: [2; 32],
            trailer_crc: 3,
        };

        assert!(header.validate().is_ok());
        assert!(trailer.validate().is_ok());
    }
}
