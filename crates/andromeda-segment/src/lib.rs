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

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::Lsn;

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
}
