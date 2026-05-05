//! GPU execution capabilities and policies.
//!
//! This module defines GPU availability, execution policies, and validation
//! of which pipeline classes are permitted to use GPU resources.

use super::hardware_pipeline::PipelineClass;
use crate::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Policy for GPU execution availability and restrictions.
///
/// Determines whether GPU can be used and on which pipeline types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuExecutionPolicy {
    /// GPU is not available for any use
    Disabled,
    /// GPU may be used for off-critical-path work (background, analytics)
    OffCriticalPathOnly,
    /// GPU may only be used for batch analytics work
    BatchAnalyticsOnly,
}

impl GpuExecutionPolicy {
    /// Returns true if the GPU execution policy permits the given pipeline.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let policy = GpuExecutionPolicy::OffCriticalPathOnly;
    /// assert!(!policy.permits_pipeline(PipelineClass::Commit));
    /// ```
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

    /// Validates that GPU execution is permitted for the given pipeline.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU execution is not permitted for this pipeline.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let policy = GpuExecutionPolicy::BatchAnalyticsOnly;
    /// policy.validate_pipeline(PipelineClass::Commit)?;
    /// ```
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
///
/// Specifies whether GPU is available and what execution policies are in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuProfile {
    /// Whether GPU hardware is available
    pub available: bool,
    /// Execution policy for available GPU
    pub execution_policy: GpuExecutionPolicy,
}

impl GpuProfile {
    /// Creates a GPU profile with GPU disabled.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let profile = GpuProfile::disabled();
    /// ```
    pub const fn disabled() -> Self {
        Self {
            available: false,
            execution_policy: GpuExecutionPolicy::Disabled,
        }
    }

    /// Creates a GPU profile with GPU available for off-critical-path work.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let profile = GpuProfile::off_critical_path();
    /// ```
    pub const fn off_critical_path() -> Self {
        Self {
            available: true,
            execution_policy: GpuExecutionPolicy::OffCriticalPathOnly,
        }
    }

    /// Creates a GPU profile restricted to batch analytics operations.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let profile = GpuProfile::batch_analytics_only();
    /// ```
    pub const fn batch_analytics_only() -> Self {
        Self {
            available: true,
            execution_policy: GpuExecutionPolicy::BatchAnalyticsOnly,
        }
    }

    /// Validates that GPU execution is available and permitted for the pipeline.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU is not available or the pipeline is not permitted.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let profile = GpuProfile::disabled();
    /// assert!(profile.validate_pipeline(PipelineClass::BatchAnalytics).is_err());
    /// ```
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
