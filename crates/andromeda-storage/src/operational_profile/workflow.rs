use andromeda_core::AndromedaResult;

use crate::{HotColdIoThresholds, IoPathClass, IoUseClass, PageIoBudget, SegmentIoBudget};

use super::{
    budgets::{cold_segment_budget, hot_page_budget, hot_segment_budget},
    constants::{COLD_SEGMENT_BYTES, HOT_SEGMENT_BYTES, MIB},
    errors::storage_error,
};

/// Named operational IO presets for storage workflow validation.
///
/// Presets are guardrail contracts only. They do not report benchmarks or make
/// claims about device-specific throughput or latency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalProfileMode {
    Conservative,
    HotWrite,
    ColdArchive,
    AnalyticsOffCriticalPath,
}

/// Storage IO workflow section of an operational preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoWorkflowProfile {
    pub mode: OperationalProfileMode,
    pub page_budget: PageIoBudget,
    pub segment_budget: SegmentIoBudget,
    pub thresholds: HotColdIoThresholds,
}

impl IoWorkflowProfile {
    pub const fn conservative() -> Self {
        let thresholds = HotColdIoThresholds::conservative();
        Self {
            mode: OperationalProfileMode::Conservative,
            page_budget: hot_page_budget(5_000, 5_000, 10_000, 64 * MIB),
            segment_budget: hot_segment_budget(
                HOT_SEGMENT_BYTES,
                5_000_000,
                5_000_000,
                10_000,
                64 * MIB,
                thresholds,
            ),
            thresholds,
        }
    }

    pub const fn hot_write() -> Self {
        let thresholds = HotColdIoThresholds::conservative();
        Self {
            mode: OperationalProfileMode::HotWrite,
            page_budget: hot_page_budget(2_000, 2_000, 5_000, 128 * MIB),
            segment_budget: hot_segment_budget(
                HOT_SEGMENT_BYTES,
                2_000_000,
                2_000_000,
                5_000,
                128 * MIB,
                thresholds,
            ),
            thresholds,
        }
    }

    pub const fn cold_archive() -> Self {
        let thresholds = HotColdIoThresholds::conservative();
        Self {
            mode: OperationalProfileMode::ColdArchive,
            page_budget: hot_page_budget(10_000, 10_000, 10_000, 64 * MIB),
            segment_budget: cold_segment_budget(
                COLD_SEGMENT_BYTES,
                5_000_000,
                5_000_000,
                5_000_000,
                64 * MIB,
                thresholds,
            ),
            thresholds,
        }
    }

    pub const fn analytics_off_critical_path() -> Self {
        let thresholds = HotColdIoThresholds::conservative();
        Self {
            mode: OperationalProfileMode::AnalyticsOffCriticalPath,
            page_budget: hot_page_budget(10_000, 10_000, 10_000, 64 * MIB),
            segment_budget: cold_segment_budget(
                COLD_SEGMENT_BYTES,
                5_000_000,
                5_000_000,
                5_000_000,
                64 * MIB,
                thresholds,
            ),
            thresholds,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.thresholds.validate()?;
        if self.segment_budget.thresholds != self.thresholds {
            return Err(storage_error(
                "workflow thresholds must match the segment IO budget thresholds",
            ));
        }

        self.page_budget.validate()?;
        self.segment_budget.validate()?;

        match self.mode {
            OperationalProfileMode::Conservative | OperationalProfileMode::HotWrite => {
                self.require_hot_page_path()?;
                self.require_hot_segment_path()?;
            }
            OperationalProfileMode::ColdArchive
            | OperationalProfileMode::AnalyticsOffCriticalPath => {
                self.require_hot_page_path()?;
                self.require_cold_segment_path()?;
            }
        }

        Ok(())
    }

    fn require_hot_page_path(&self) -> AndromedaResult<()> {
        if self.page_budget.use_class != IoUseClass::OnlineHotPath
            || self.page_budget.path_budget.path_class != IoPathClass::HotPathNvmeSsd
        {
            return Err(storage_error(
                "workflow page budget must keep online pages on the HotStore path",
            ));
        }
        Ok(())
    }

    fn require_hot_segment_path(&self) -> AndromedaResult<()> {
        if self.segment_budget.use_class != IoUseClass::CommitCriticalHotPath
            || self.segment_budget.path_budget.path_class != IoPathClass::HotPathNvmeSsd
        {
            return Err(storage_error(
                "workflow segment budget must keep commit-critical writes on HotStore",
            ));
        }
        Ok(())
    }

    fn require_cold_segment_path(&self) -> AndromedaResult<()> {
        if self.segment_budget.use_class != IoUseClass::ColdSegmentPath
            || self.segment_budget.path_budget.path_class != IoPathClass::ColdPathHdd
        {
            return Err(storage_error(
                "workflow segment budget must use the ColdStore path for archive work",
            ));
        }
        Ok(())
    }
}

impl Default for IoWorkflowProfile {
    fn default() -> Self {
        Self::conservative()
    }
}
