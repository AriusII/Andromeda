use std::num::NonZeroU64;

use andromeda_analytics::{AdvisoryAnalyticsJob, AnalyticsJobDescriptor, AnalyticsWorkloadKind};
use andromeda_columnar::{ColumnarArtifactDescriptor, ColumnarConsumer, ColumnarSegmentDescriptor};

use crate::{MapDescriptor, MapDescriptorError, MapDescriptorResult, MapPublicationCandidate};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MapRefreshPlan {
    pub descriptor: MapDescriptor,
    pub analytics_job: AnalyticsJobDescriptor,
    pub columnar_segment: Option<ColumnarSegmentDescriptor>,
}

impl MapRefreshPlan {
    pub fn new(
        descriptor: MapDescriptor,
        analytics_job: AnalyticsJobDescriptor,
        columnar_segment: Option<ColumnarSegmentDescriptor>,
    ) -> MapDescriptorResult<Self> {
        if analytics_job.workload_kind() != AnalyticsWorkloadKind::MapRefresh {
            return Err(MapDescriptorError::AnalyticsWorkloadNotMapRefresh);
        }
        if !analytics_job.is_admissible_advisory_job() {
            return Err(MapDescriptorError::AnalyticsJobNotAdvisory);
        }
        if let Some(segment) = &columnar_segment {
            if segment.consumer() != ColumnarConsumer::Maps {
                return Err(MapDescriptorError::ColumnarConsumerNotMaps);
            }
            if !segment.is_advisory_columnar_artifact() {
                return Err(MapDescriptorError::ColumnarArtifactNotAdvisory);
            }
        }

        Ok(Self {
            descriptor,
            analytics_job,
            columnar_segment,
        })
    }

    pub fn candidate(
        &self,
        catalog_version: u64,
        stats_version: u64,
    ) -> MapDescriptorResult<MapPublicationCandidate> {
        MapPublicationCandidate::new(self.descriptor, catalog_version, stats_version)
    }

    pub fn requires_decision_trace(&self) -> bool {
        self.analytics_job.requires_decision_trace()
    }

    pub const fn owns_source_truth(&self) -> bool {
        false
    }

    pub const fn owns_durable_publication_path(&self) -> bool {
        false
    }

    pub fn source_snapshot_lsn(&self) -> Option<NonZeroU64> {
        self.columnar_segment
            .as_ref()
            .map(|segment| segment.snapshot_binding().source_snapshot_lsn)
    }
}
