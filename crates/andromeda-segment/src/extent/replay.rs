use crate::SegmentId;

use super::{ColdExtentReclaimEvidence, ExtentDescriptor, ExtentId};

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
