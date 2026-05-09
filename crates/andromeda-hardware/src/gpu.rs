//! GPU availability and pipeline eligibility policy.

use crate::PipelineClass;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Policy for GPU execution availability and restrictions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuExecutionPolicy {
    Disabled,
    OffCriticalPathOnly,
    BatchAnalyticsOnly,
}

impl GpuExecutionPolicy {
    pub const fn permits_pipeline(self, pipeline: PipelineClass) -> bool {
        match self {
            Self::Disabled => false,
            Self::OffCriticalPathOnly => {
                !pipeline.is_critical_path() && pipeline.is_gpu_advisory_candidate()
            },
            Self::BatchAnalyticsOnly => matches!(
                pipeline,
                PipelineClass::StatisticsRefresh
                    | PipelineClass::MapRefresh
                    | PipelineClass::BatchAnalytics
            ),
        }
    }

    pub fn validate_pipeline(self, pipeline: PipelineClass) -> AndromedaResult<()> {
        if self.permits_pipeline(pipeline) {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            format!(
                "GPU execution is not permitted for {} pipeline",
                pipeline.name()
            ),
        ))
    }
}

/// GPU profile describing GPU availability and constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuProfile {
    pub available: bool,
    pub execution_policy: GpuExecutionPolicy,
}

impl GpuProfile {
    pub const fn disabled() -> Self {
        Self {
            available: false,
            execution_policy: GpuExecutionPolicy::Disabled,
        }
    }

    /// Allows GPU only outside critical engine truth paths.
    pub const fn off_critical_path() -> Self {
        Self {
            available: true,
            execution_policy: GpuExecutionPolicy::OffCriticalPathOnly,
        }
    }

    /// Allows GPU only for analytical batch-style pipelines.
    pub const fn batch_analytics_only() -> Self {
        Self {
            available: true,
            execution_policy: GpuExecutionPolicy::BatchAnalyticsOnly,
        }
    }

    pub fn validate_pipeline(&self, pipeline: PipelineClass) -> AndromedaResult<()> {
        if !self.available {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "GPU execution is not available",
            ));
        }

        self.execution_policy.validate_pipeline(pipeline)
    }

    /// Selects advisory GPU execution only when CPU fallback and cancellation are explicit.
    pub fn select_advisory_gpu(
        &self,
        pipeline: PipelineClass,
        cpu_fallback_available: bool,
        cancellation_boundary: bool,
    ) -> AndromedaResult<bool> {
        if !pipeline.is_gpu_advisory_candidate() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                format!(
                    "GPU advisory execution is not permitted for {} pipeline",
                    pipeline.name()
                ),
            ));
        }

        if !cpu_fallback_available {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "GPU advisory execution requires authoritative CPU fallback",
            ));
        }

        if !cancellation_boundary {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "GPU advisory execution requires a cancellation boundary",
            ));
        }

        if !self.available || self.execution_policy == GpuExecutionPolicy::Disabled {
            return Ok(false);
        }

        self.execution_policy.validate_pipeline(pipeline)?;
        Ok(true)
    }
}

impl Default for GpuProfile {
    fn default() -> Self {
        Self::disabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRITICAL_PIPELINES: [PipelineClass; 7] = [
        PipelineClass::Commit,
        PipelineClass::WalAppend,
        PipelineClass::Rollback,
        PipelineClass::Recovery,
        PipelineClass::MvccVisibility,
        PipelineClass::CatalogPublication,
        PipelineClass::SecurityCriticalPath,
    ];
    const ANALYTICS_PIPELINES: [PipelineClass; 3] = [
        PipelineClass::StatisticsRefresh,
        PipelineClass::MapRefresh,
        PipelineClass::BatchAnalytics,
    ];
    const NON_ADVISORY_PIPELINES: [PipelineClass; 2] = [
        PipelineClass::ForegroundExecution,
        PipelineClass::BackgroundMaintenance,
    ];

    #[test]
    fn gpu_policy_rejects_critical_engine_truth_paths() {
        let policy = GpuExecutionPolicy::OffCriticalPathOnly;

        for pipeline in CRITICAL_PIPELINES {
            let error = policy.validate_pipeline(pipeline).unwrap_err();
            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
            assert!(!policy.permits_pipeline(pipeline));
        }
    }

    #[test]
    fn off_critical_policy_rejects_non_advisory_pipelines() {
        let policy = GpuExecutionPolicy::OffCriticalPathOnly;

        for pipeline in NON_ADVISORY_PIPELINES {
            let error = policy.validate_pipeline(pipeline).unwrap_err();
            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
            assert!(!policy.permits_pipeline(pipeline));
        }

        for pipeline in ANALYTICS_PIPELINES {
            assert!(policy.validate_pipeline(pipeline).is_ok());
        }
    }

    #[test]
    fn batch_analytics_policy_only_allows_analytical_gpu_work() {
        let policy = GpuExecutionPolicy::BatchAnalyticsOnly;

        for pipeline in ANALYTICS_PIPELINES {
            assert!(policy.validate_pipeline(pipeline).is_ok());
        }

        for pipeline in CRITICAL_PIPELINES.into_iter().chain(NON_ADVISORY_PIPELINES) {
            assert!(policy.validate_pipeline(pipeline).is_err());
        }
    }

    #[test]
    fn gpu_batch_analytics_profile_rejects_foreground_and_critical_work() {
        let profile = GpuProfile::batch_analytics_only();

        assert!(
            profile
                .validate_pipeline(PipelineClass::StatisticsRefresh)
                .is_ok()
        );
        assert!(profile.validate_pipeline(PipelineClass::MapRefresh).is_ok());
        assert!(
            profile
                .validate_pipeline(PipelineClass::BatchAnalytics)
                .is_ok()
        );

        for pipeline in CRITICAL_PIPELINES.into_iter().chain(NON_ADVISORY_PIPELINES) {
            assert!(profile.validate_pipeline(pipeline).is_err());
        }
    }

    #[test]
    fn advisory_gpu_selection_requires_cpu_fallback() {
        let profile = GpuProfile::batch_analytics_only();

        let error = profile
            .select_advisory_gpu(PipelineClass::BatchAnalytics, false, true)
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("CPU fallback"));
    }

    #[test]
    fn advisory_gpu_selection_requires_cancellation_boundary() {
        let profile = GpuProfile::batch_analytics_only();

        let error = profile
            .select_advisory_gpu(PipelineClass::BatchAnalytics, true, false)
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("cancellation boundary"));
    }

    #[test]
    fn advisory_gpu_selection_uses_cpu_when_gpu_is_disabled() {
        let profile = GpuProfile::disabled();

        let selected = profile
            .select_advisory_gpu(PipelineClass::BatchAnalytics, true, true)
            .unwrap();

        assert!(!selected);
    }

    #[test]
    fn advisory_gpu_selection_rejects_critical_truth_paths() {
        let profile = GpuProfile::batch_analytics_only();

        for pipeline in CRITICAL_PIPELINES {
            let error = profile
                .select_advisory_gpu(pipeline, true, true)
                .unwrap_err();
            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
            assert!(!profile.execution_policy.permits_pipeline(pipeline));
        }
    }
}
