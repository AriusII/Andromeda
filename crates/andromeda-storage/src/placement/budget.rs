use andromeda_core::AndromedaResult;

use crate::PageSize;

use super::{HotColdIoThresholds, IoPathBudget, IoUseClass, PageIoBudget, SegmentIoBudget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageIoBudgetScope {
    Page(PageSize),
    Segment { bytes: u64 },
}

impl StorageIoBudgetScope {
    pub const fn logical_bytes(self) -> u64 {
        match self {
            Self::Page(page_size) => page_size.bytes() as u64,
            Self::Segment { bytes } => bytes,
        }
    }

    pub(super) fn validate(
        self,
        use_class: IoUseClass,
        path_budget: IoPathBudget,
        thresholds: HotColdIoThresholds,
    ) -> AndromedaResult<()> {
        match self {
            Self::Page(page_size) => {
                PageIoBudget::new(page_size, use_class, path_budget).validate()
            },
            Self::Segment { bytes } => {
                SegmentIoBudget::new(bytes, use_class, path_budget, thresholds).validate()
            },
        }
    }
}
