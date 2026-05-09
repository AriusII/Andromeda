use andromeda_analytics::{AdvisoryAnalyticsJob, AnalyticsJobDescriptor, AnalyticsWorkloadKind};
use andromeda_columnar::{ColumnarArtifactDescriptor, ColumnarConsumer, ColumnarLayoutDescriptor};

use crate::{MapDescriptor, MapDescriptorError, MapDescriptorResult, MapPublicationCandidate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapRefreshPlan {
    pub descriptor: MapDescriptor,
    pub analytics_job: AnalyticsJobDescriptor,
    pub columnar_layout: Option<ColumnarLayoutDescriptor>,
}

impl MapRefreshPlan {
    pub fn new(
        descriptor: MapDescriptor,
        analytics_job: AnalyticsJobDescriptor,
        columnar_layout: Option<ColumnarLayoutDescriptor>,
    ) -> MapDescriptorResult<Self> {
        if analytics_job.workload_kind() != AnalyticsWorkloadKind::MapRefresh {
            return Err(MapDescriptorError::AnalyticsWorkloadNotMapRefresh);
        }
        if !analytics_job.is_admissible_advisory_job() {
            return Err(MapDescriptorError::AnalyticsJobNotAdvisory);
        }
        if let Some(layout) = columnar_layout {
            if layout.consumer() != ColumnarConsumer::Maps {
                return Err(MapDescriptorError::ColumnarConsumerNotMaps);
            }
            if !layout.is_advisory_columnar_artifact() {
                return Err(MapDescriptorError::ColumnarArtifactNotAdvisory);
            }
        }

        Ok(Self {
            descriptor,
            analytics_job,
            columnar_layout,
        })
    }

    pub fn candidate(
        self,
        catalog_version: u64,
        stats_version: u64,
    ) -> MapDescriptorResult<MapPublicationCandidate> {
        MapPublicationCandidate::new(self.descriptor, catalog_version, stats_version)
    }

    pub fn requires_decision_trace(self) -> bool {
        self.analytics_job.requires_decision_trace()
    }

    pub const fn owns_source_truth(self) -> bool {
        false
    }

    pub const fn owns_durable_publication_path(self) -> bool {
        false
    }
}
