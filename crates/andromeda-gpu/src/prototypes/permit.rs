//! Prevalidated execution permits for prototype GPU pipelines.

use andromeda_error::AndromedaResult;
use andromeda_hardware::OptionalGpuSelection;

use crate::policy::{
    GpuProfile, OptionalGpuDecision, OptionalGpuRequest, PipelineClass, select_optional_gpu,
};

/// Prevalidated execution permit for `GPU_STATS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsExecutionPermit {
    decision: OptionalGpuDecision,
}

impl StatsExecutionPermit {
    /// Prevalidates policy for `PipelineClass::StatisticsRefresh`.
    pub fn prevalidate(profile: GpuProfile) -> AndromedaResult<Self> {
        let request = OptionalGpuRequest::new(PipelineClass::StatisticsRefresh)
            .with_cpu_fallback()
            .with_cancellation_boundary();
        let decision = select_optional_gpu(profile, request)?;
        Ok(Self { decision })
    }

    pub(crate) fn gpu_selected(&self) -> bool {
        matches!(self.decision.selection, OptionalGpuSelection::AdvisoryGpu)
    }
}

/// Prevalidated execution permit for `GPU_ANALYTICS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalyticsExecutionPermit {
    decision: OptionalGpuDecision,
}

impl AnalyticsExecutionPermit {
    /// Prevalidates policy for `PipelineClass::BatchAnalytics`.
    pub fn prevalidate(profile: GpuProfile) -> AndromedaResult<Self> {
        let request = OptionalGpuRequest::new(PipelineClass::BatchAnalytics)
            .with_cpu_fallback()
            .with_cancellation_boundary();
        let decision = select_optional_gpu(profile, request)?;
        Ok(Self { decision })
    }

    pub(crate) fn gpu_selected(&self) -> bool {
        matches!(self.decision.selection, OptionalGpuSelection::AdvisoryGpu)
    }
}
