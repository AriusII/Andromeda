use std::num::NonZeroU64;

use andromeda_analytics::{AdvisoryAnalyticsJob, AnalyticsAccelerationPolicy};
use andromeda_columnar::{ColumnarArtifactDescriptor, ColumnarSegmentDescriptor};

use crate::{
    MapDescriptor, MapDescriptorError, MapDescriptorResult, MapPublicationCandidate,
    MapRefreshMode, MapRefreshPlan, MapStalenessPolicy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapConsistencyPolicy {
    pub descriptor: MapDescriptor,
    pub max_refresh_cost: NonZeroU64,
    pub max_wal_queue_depth: NonZeroU64,
}

impl MapConsistencyPolicy {
    pub const fn new(
        descriptor: MapDescriptor,
        max_refresh_cost: NonZeroU64,
        max_wal_queue_depth: NonZeroU64,
    ) -> Self {
        Self {
            descriptor,
            max_refresh_cost,
            max_wal_queue_depth,
        }
    }

    pub const fn admits_staleness(self, observed_staleness_millis: u64) -> bool {
        match self.descriptor.staleness {
            MapStalenessPolicy::CurrentOnly => observed_staleness_millis == 0,
            MapStalenessPolicy::BoundedMillis(max_age_millis) => {
                observed_staleness_millis <= max_age_millis
            },
            MapStalenessPolicy::AdvisorySnapshot => true,
        }
    }

    pub const fn preserves_wal_priority(self, wal_queue_depth: u64) -> bool {
        wal_queue_depth <= self.max_wal_queue_depth.get()
    }

    pub fn admit(
        self,
        plan: &MapRefreshPlan,
        request: MapRefreshAdmissionRequest,
        catalog_version: u64,
        stats_version: u64,
    ) -> MapDescriptorResult<MapRefreshAdmissionDecision> {
        if plan.descriptor != self.descriptor {
            return Err(MapDescriptorError::ConsistencyPolicyDescriptorMismatch);
        }
        if request.refresh_cost == 0 || request.refresh_cost > self.max_refresh_cost.get() {
            return Err(MapDescriptorError::RefreshCostExceedsBudget);
        }
        if !self.admits_staleness(request.observed_staleness_millis) {
            return Err(MapDescriptorError::StalenessExceeded);
        }
        if !request.decision_trace_emitted {
            return Err(MapDescriptorError::MissingDecisionTrace);
        }
        if !self.preserves_wal_priority(request.wal_queue_depth) {
            return Err(MapDescriptorError::WalPriorityExceeded);
        }
        if !is_cpu_first_plan(plan) {
            return Err(MapDescriptorError::RefreshPathNotCpuFirst);
        }

        let (wal_records_reserved, source_snapshot_lsn, delta_log) =
            self.validate_mode_specific_requirements(plan, request)?;

        Ok(MapRefreshAdmissionDecision {
            candidate: plan.candidate(catalog_version, stats_version)?,
            refresh_cost: request.refresh_cost,
            wal_records_reserved,
            source_snapshot_lsn,
            delta_log,
        })
    }

    fn validate_mode_specific_requirements(
        self,
        plan: &MapRefreshPlan,
        request: MapRefreshAdmissionRequest,
    ) -> MapDescriptorResult<(u64, Option<NonZeroU64>, Option<MapDeltaLog>)> {
        let source_snapshot_lsn = plan.source_snapshot_lsn();

        match self.descriptor.refresh_mode {
            MapRefreshMode::Immediate => {
                if request.table_wal_records == 0 {
                    return Err(MapDescriptorError::ImmediateRefreshMissingTableWalRecords);
                }
                if request.map_wal_records == 0 {
                    return Err(MapDescriptorError::ImmediateRefreshMissingMapWalRecords);
                }

                Ok((
                    request.table_wal_records + request.map_wal_records,
                    source_snapshot_lsn,
                    None,
                ))
            },
            MapRefreshMode::Incremental => {
                let delta_log = request
                    .delta_log
                    .ok_or(MapDescriptorError::IncrementalRefreshMissingDeltaLog)?;
                if !delta_log.is_controlled_for(self.descriptor) {
                    return Err(MapDescriptorError::IncrementalDeltaLogMapMismatch);
                }

                Ok((
                    delta_log.total_wal_records(),
                    source_snapshot_lsn,
                    Some(delta_log),
                ))
            },
            MapRefreshMode::Deferred => Ok((
                request.table_wal_records + request.map_wal_records,
                source_snapshot_lsn,
                None,
            )),
            MapRefreshMode::SnapshotOnly => {
                let published_snapshot_lsn = request
                    .published_snapshot_lsn
                    .ok_or(MapDescriptorError::SnapshotRefreshRequiresPublishedSnapshot)?;
                let segment = plan
                    .columnar_segment
                    .as_ref()
                    .ok_or(MapDescriptorError::SnapshotRefreshRequiresPublishedSnapshot)?;
                if !segment.is_bound_to_source_snapshot(published_snapshot_lsn) {
                    return Err(MapDescriptorError::SnapshotRefreshSnapshotMismatch);
                }

                Ok((0, Some(published_snapshot_lsn), None))
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapDeltaLog {
    pub descriptor: MapDescriptor,
    pub base_snapshot_lsn: NonZeroU64,
    pub delta_lsn: NonZeroU64,
    pub table_wal_records: NonZeroU64,
    pub map_wal_records: NonZeroU64,
}

impl MapDeltaLog {
    /// Constructs a `MapDeltaLog`.
    ///
    /// # Ordering invariant
    /// `delta_lsn` MUST be strictly greater than `base_snapshot_lsn`.  A delta window
    /// where `delta_lsn <= base_snapshot_lsn` is temporally inverted and is rejected
    /// with [`MapDescriptorError::InvertedDeltaLsnWindow`].
    pub fn new(
        descriptor: MapDescriptor,
        base_snapshot_lsn: NonZeroU64,
        delta_lsn: NonZeroU64,
        table_wal_records: NonZeroU64,
        map_wal_records: NonZeroU64,
    ) -> crate::MapDescriptorResult<Self> {
        if delta_lsn.get() <= base_snapshot_lsn.get() {
            return Err(crate::MapDescriptorError::InvertedDeltaLsnWindow);
        }
        Ok(Self {
            descriptor,
            base_snapshot_lsn,
            delta_lsn,
            table_wal_records,
            map_wal_records,
        })
    }

    pub const fn total_wal_records(self) -> u64 {
        self.table_wal_records.get() + self.map_wal_records.get()
    }

    pub fn is_controlled_for(self, descriptor: MapDescriptor) -> bool {
        self.descriptor == descriptor
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapRefreshAdmissionRequest {
    pub refresh_cost: u64,
    pub observed_staleness_millis: u64,
    pub table_wal_records: u64,
    pub map_wal_records: u64,
    pub wal_queue_depth: u64,
    pub decision_trace_emitted: bool,
    pub published_snapshot_lsn: Option<NonZeroU64>,
    pub delta_log: Option<MapDeltaLog>,
}

impl MapRefreshAdmissionRequest {
    pub const fn new(
        refresh_cost: u64,
        observed_staleness_millis: u64,
        wal_queue_depth: u64,
    ) -> Self {
        Self {
            refresh_cost,
            observed_staleness_millis,
            table_wal_records: 0,
            map_wal_records: 0,
            wal_queue_depth,
            decision_trace_emitted: false,
            published_snapshot_lsn: None,
            delta_log: None,
        }
    }

    pub const fn with_table_wal_records(mut self, table_wal_records: u64) -> Self {
        self.table_wal_records = table_wal_records;
        self
    }

    pub const fn with_map_wal_records(mut self, map_wal_records: u64) -> Self {
        self.map_wal_records = map_wal_records;
        self
    }

    pub const fn with_decision_trace(mut self) -> Self {
        self.decision_trace_emitted = true;
        self
    }

    pub const fn with_published_snapshot_lsn(mut self, published_snapshot_lsn: NonZeroU64) -> Self {
        self.published_snapshot_lsn = Some(published_snapshot_lsn);
        self
    }

    pub const fn with_delta_log(mut self, delta_log: MapDeltaLog) -> Self {
        self.delta_log = Some(delta_log);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapRefreshAdmissionDecision {
    pub candidate: MapPublicationCandidate,
    pub refresh_cost: u64,
    pub wal_records_reserved: u64,
    pub source_snapshot_lsn: Option<NonZeroU64>,
    pub delta_log: Option<MapDeltaLog>,
}

impl MapRefreshAdmissionDecision {
    pub const fn preserves_wal_priority(self) -> bool {
        true
    }

    pub const fn uses_incremental_delta(self) -> bool {
        self.delta_log.is_some()
    }
}

fn is_cpu_first_plan(plan: &MapRefreshPlan) -> bool {
    if plan.analytics_job.acceleration_policy() != AnalyticsAccelerationPolicy::CpuOnly {
        return false;
    }

    plan.columnar_segment.as_ref().is_none_or(cpu_only_segment)
}

fn cpu_only_segment(segment: &ColumnarSegmentDescriptor) -> bool {
    segment.acceleration_policy() == andromeda_columnar::ColumnarAccelerationPolicy::CpuOnly
}

// ─── Idempotency guard ────────────────────────────────────────────────────────

/// Outcome of a [`MapDeltaApplyState::try_apply`] call.
///
/// - `Applied` — the delta advanced the apply cursor; `new_applied_up_to_lsn` is the new frontier.
/// - `AlreadyApplied` — the delta window was already covered by a prior apply; this is the
///   idempotent-ack path used during WAL replay and crash recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapDeltaApplyOutcome {
    /// Delta was freshly applied; the apply cursor advanced to `new_applied_up_to_lsn`.
    Applied { new_applied_up_to_lsn: NonZeroU64 },
    /// Delta window was already applied — this call is a no-op replay-safe ack.
    AlreadyApplied,
}

/// Tracks the applied-up-to LSN for a single Map's incremental delta application.
///
/// Enforces the idempotency invariant: re-applying a delta whose `delta_lsn` is already
/// covered by `applied_up_to_lsn` returns [`MapDeltaApplyOutcome::AlreadyApplied`] rather
/// than advancing the cursor a second time.
///
/// # Crash recovery
///
/// `MapDeltaApplyState` is **in-memory only** and is never the source of truth.  After a
/// crash, the caller reconstructs apply state from the last durable
/// [`crate::MapPublicationRecoveryEvidence`] and may safely re-apply any delta whose
/// `delta_lsn > publication_lsn` of the recovered evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapDeltaApplyState {
    pub descriptor: MapDescriptor,
    /// The highest `delta_lsn` that has been successfully applied.  `None` means the
    /// Map has never had an incremental delta applied in this session.
    pub applied_up_to_lsn: Option<NonZeroU64>,
}

impl MapDeltaApplyState {
    /// Creates a fresh apply state for `descriptor` with no prior applications.
    pub const fn new(descriptor: MapDescriptor) -> Self {
        Self {
            descriptor,
            applied_up_to_lsn: None,
        }
    }

    /// Creates an apply state pre-seeded from a recovered durable publication LSN.
    ///
    /// Use this after crash recovery to avoid re-applying deltas that were already
    /// committed.  `recovered_lsn` should be the `publication_lsn` (or equivalent
    /// durable frontier) from the recovery evidence.
    pub const fn recovered(descriptor: MapDescriptor, recovered_lsn: NonZeroU64) -> Self {
        Self {
            descriptor,
            applied_up_to_lsn: Some(recovered_lsn),
        }
    }

    /// Attempt to apply `delta_log` to this state.
    ///
    /// Returns:
    /// - `Ok(Applied { .. })` — delta was freshly applied and the cursor advanced.
    /// - `Ok(AlreadyApplied)` — delta window is entirely covered by the current frontier;
    ///   this is an idempotent no-op (safe to replay).
    /// - `Err(IncrementalDeltaLogMapMismatch)` — `delta_log.descriptor` does not match
    ///   `self.descriptor`.
    pub fn try_apply(
        &mut self,
        delta_log: MapDeltaLog,
    ) -> crate::MapDescriptorResult<MapDeltaApplyOutcome> {
        if delta_log.descriptor != self.descriptor {
            return Err(crate::MapDescriptorError::IncrementalDeltaLogMapMismatch);
        }

        // Idempotency: if the delta_lsn is already covered by the applied frontier,
        // treat this as a replay-safe no-op rather than a duplicate apply.
        if let Some(applied) = self.applied_up_to_lsn
            && delta_log.delta_lsn.get() <= applied.get()
        {
            return Ok(MapDeltaApplyOutcome::AlreadyApplied);
        }

        self.applied_up_to_lsn = Some(delta_log.delta_lsn);
        Ok(MapDeltaApplyOutcome::Applied {
            new_applied_up_to_lsn: delta_log.delta_lsn,
        })
    }
}
