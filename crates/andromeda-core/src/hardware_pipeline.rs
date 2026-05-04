//! Pipeline classification for different execution contexts.
//!
//! This module categorizes different types of database operations into pipeline classes
//! that determine which resources and policies apply to their execution.

/// Classification of operational pipelines by execution context and criticality.
///
/// Different pipeline types have different resource and GPU constraints,
/// prioritization, and failure recovery requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineClass {
    /// Transaction commit operations (critical path)
    Commit,
    /// Write-ahead log (WAL) append (critical path)
    WalAppend,
    /// Transaction rollback (critical path)
    Rollback,
    /// Recovery from crash or interruption (critical path)
    Recovery,
    /// User-facing query execution (foreground)
    ForegroundExecution,
    /// Background maintenance operations
    BackgroundMaintenance,
    /// Statistics and histogram collection
    StatisticsRefresh,
    /// Bloom filter or index map refresh
    MapRefresh,
    /// Batch analytics and reporting queries
    BatchAnalytics,
}

impl PipelineClass {
    /// Returns true if this pipeline is part of the critical path.
    ///
    /// Critical path pipelines must complete successfully to maintain
    /// database consistency and durability. They have strict constraints:
    /// - GPU is never allowed
    /// - Must not be preempted or delayed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert!(PipelineClass::Commit.is_critical_path());
    /// assert!(!PipelineClass::BatchAnalytics.is_critical_path());
    /// ```
    pub const fn is_critical_path(self) -> bool {
        matches!(
            self,
            Self::Commit | Self::WalAppend | Self::Rollback | Self::Recovery
        )
    }

    /// Returns the stable string name for this pipeline class.
    ///
    /// Used in logging, metrics, and error messages.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert_eq!(PipelineClass::BatchAnalytics.name(), "batch_analytics");
    /// ```
    pub const fn name(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::WalAppend => "wal_append",
            Self::Rollback => "rollback",
            Self::Recovery => "recovery",
            Self::ForegroundExecution => "foreground_execution",
            Self::BackgroundMaintenance => "background_maintenance",
            Self::StatisticsRefresh => "statistics_refresh",
            Self::MapRefresh => "map_refresh",
            Self::BatchAnalytics => "batch_analytics",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_path_pipeline_identification() {
        assert!(PipelineClass::Commit.is_critical_path());
        assert!(PipelineClass::WalAppend.is_critical_path());
        assert!(PipelineClass::Rollback.is_critical_path());
        assert!(PipelineClass::Recovery.is_critical_path());

        assert!(!PipelineClass::ForegroundExecution.is_critical_path());
        assert!(!PipelineClass::BackgroundMaintenance.is_critical_path());
        assert!(!PipelineClass::StatisticsRefresh.is_critical_path());
        assert!(!PipelineClass::MapRefresh.is_critical_path());
        assert!(!PipelineClass::BatchAnalytics.is_critical_path());
    }

    #[test]
    fn pipeline_names_are_stable() {
        assert_eq!(PipelineClass::Commit.name(), "commit");
        assert_eq!(PipelineClass::WalAppend.name(), "wal_append");
        assert_eq!(PipelineClass::Rollback.name(), "rollback");
        assert_eq!(PipelineClass::Recovery.name(), "recovery");
        assert_eq!(
            PipelineClass::ForegroundExecution.name(),
            "foreground_execution"
        );
        assert_eq!(
            PipelineClass::BackgroundMaintenance.name(),
            "background_maintenance"
        );
        assert_eq!(
            PipelineClass::StatisticsRefresh.name(),
            "statistics_refresh"
        );
        assert_eq!(PipelineClass::MapRefresh.name(), "map_refresh");
        assert_eq!(PipelineClass::BatchAnalytics.name(), "batch_analytics");
    }
}
