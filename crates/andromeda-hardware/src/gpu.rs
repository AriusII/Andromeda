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
            Self::OffCriticalPathOnly => !pipeline.is_critical_path(),
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

    /// Allows GPU only outside critical commit/recovery paths.
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
}

impl Default for GpuProfile {
    fn default() -> Self {
        Self::disabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_policy_rejects_commit_wal_rollback_and_recovery() {
        let policy = GpuExecutionPolicy::OffCriticalPathOnly;

        for pipeline in [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
        ] {
            let error = policy.validate_pipeline(pipeline).unwrap_err();
            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
            assert!(!policy.permits_pipeline(pipeline));
        }
    }

    #[test]
    fn batch_analytics_policy_only_allows_analytical_gpu_work() {
        let policy = GpuExecutionPolicy::BatchAnalyticsOnly;

        for pipeline in [
            PipelineClass::StatisticsRefresh,
            PipelineClass::MapRefresh,
            PipelineClass::BatchAnalytics,
        ] {
            assert!(policy.validate_pipeline(pipeline).is_ok());
        }

        for pipeline in [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
            PipelineClass::ForegroundExecution,
            PipelineClass::BackgroundMaintenance,
        ] {
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

        for pipeline in [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
            PipelineClass::ForegroundExecution,
            PipelineClass::BackgroundMaintenance,
        ] {
            assert!(profile.validate_pipeline(pipeline).is_err());
        }
    }
}
