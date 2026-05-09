//! Re-export surface for the canonical segment value types defined in
//! `crate::wal_segment`. Do not define `WalSegment` or `WalSegmentDescriptor`
//! here.

pub use crate::{WalSegment, WalSegmentDescriptor};
