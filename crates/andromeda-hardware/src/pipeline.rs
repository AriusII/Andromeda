//! Pipeline classification for resource and GPU policy checks.

/// Classification of operational pipelines by execution context and criticality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineClass {
    Commit,
    WalAppend,
    Rollback,
    Recovery,
    MvccVisibility,
    CatalogPublication,
    SecurityCriticalPath,
    ForegroundExecution,
    BackgroundMaintenance,
    StatisticsRefresh,
    MapRefresh,
    BatchAnalytics,
}

impl PipelineClass {
    /// Critical engine truth paths stay off accelerated paths.
    pub const fn is_critical_path(self) -> bool {
        matches!(
            self,
            Self::Commit
                | Self::WalAppend
                | Self::Rollback
                | Self::Recovery
                | Self::MvccVisibility
                | Self::CatalogPublication
                | Self::SecurityCriticalPath
        )
    }

    /// GPU work may only be considered for advisory analytical pipelines.
    pub const fn is_gpu_advisory_candidate(self) -> bool {
        matches!(
            self,
            Self::StatisticsRefresh | Self::MapRefresh | Self::BatchAnalytics
        )
    }

    /// Returns the stable string name for this pipeline class.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::WalAppend => "wal_append",
            Self::Rollback => "rollback",
            Self::Recovery => "recovery",
            Self::MvccVisibility => "mvcc_visibility",
            Self::CatalogPublication => "catalog_publication",
            Self::SecurityCriticalPath => "security_critical_path",
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
        assert!(PipelineClass::MvccVisibility.is_critical_path());
        assert!(PipelineClass::CatalogPublication.is_critical_path());
        assert!(PipelineClass::SecurityCriticalPath.is_critical_path());

        assert!(!PipelineClass::ForegroundExecution.is_critical_path());
        assert!(!PipelineClass::BackgroundMaintenance.is_critical_path());
        assert!(!PipelineClass::StatisticsRefresh.is_critical_path());
        assert!(!PipelineClass::MapRefresh.is_critical_path());
        assert!(!PipelineClass::BatchAnalytics.is_critical_path());
    }

    #[test]
    fn gpu_advisory_candidates_are_limited_to_analytical_pipelines() {
        for pipeline in [
            PipelineClass::StatisticsRefresh,
            PipelineClass::MapRefresh,
            PipelineClass::BatchAnalytics,
        ] {
            assert!(pipeline.is_gpu_advisory_candidate());
        }

        for pipeline in [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
            PipelineClass::MvccVisibility,
            PipelineClass::CatalogPublication,
            PipelineClass::SecurityCriticalPath,
            PipelineClass::ForegroundExecution,
            PipelineClass::BackgroundMaintenance,
        ] {
            assert!(!pipeline.is_gpu_advisory_candidate());
        }
    }

    #[test]
    fn pipeline_names_are_stable() {
        assert_eq!(PipelineClass::Commit.name(), "commit");
        assert_eq!(PipelineClass::WalAppend.name(), "wal_append");
        assert_eq!(PipelineClass::Rollback.name(), "rollback");
        assert_eq!(PipelineClass::Recovery.name(), "recovery");
        assert_eq!(PipelineClass::MvccVisibility.name(), "mvcc_visibility");
        assert_eq!(
            PipelineClass::CatalogPublication.name(),
            "catalog_publication"
        );
        assert_eq!(
            PipelineClass::SecurityCriticalPath.name(),
            "security_critical_path"
        );
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
