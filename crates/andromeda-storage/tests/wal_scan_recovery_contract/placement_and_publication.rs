use crate::support::{manifest, record, segment};
use andromeda_core::{AndromedaErrorKind, PipelineClass, TransactionId};
use andromeda_storage::write_ahead_log::record::WalRecordKind;
use andromeda_storage::{
    CoreIoPlacementPolicy, CoreIoPlacementRequest, InMemoryWal, IoPathClass, IoUseClass, Lsn,
    OperationalProfile, PageSize, PipelineStage, PlacementDecision, PublishedColdSegment,
    RecoveryPlan, SegmentMutation, SegmentState, StartupMode, StorageIoBudgetScope, StorageTier,
    StorageWorkloadClass,
};

#[test]
fn wal_append_and_recovery_placement_stay_on_hotstore_critical_path() {
    let committed = TransactionId::new(141);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(committed).unwrap();
    let replay_lsn = wal
        .append_payload(
            WalRecordKind::RowUpdate,
            Some(committed),
            b"Inventory:item=sku-12,delta=-1",
        )
        .unwrap();
    wal.append_tx_commit(committed).unwrap();
    wal.flush_all().unwrap();

    let durable_records = wal.replay_durable();
    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &durable_records,
    )
    .unwrap();
    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![replay_lsn]);

    let profile = OperationalProfile::hot_write();
    profile.validate().unwrap();
    let policy = CoreIoPlacementPolicy::new(profile.hardware, profile.workflow.thresholds);
    let hot_budget = profile.workflow.segment_budget.path_budget;

    for (workload, pipeline) in [
        (StorageWorkloadClass::WalAppend, PipelineClass::WalAppend),
        (StorageWorkloadClass::Recovery, PipelineClass::Recovery),
    ] {
        let decision = policy
            .plan(CoreIoPlacementRequest::new(
                workload,
                StorageIoBudgetScope::Page(PageSize::KiB16),
                hot_budget,
                false,
            ))
            .unwrap();

        assert_eq!(decision.pipeline_class, pipeline);
        assert_eq!(decision.io_use_class, IoUseClass::CommitCriticalHotPath);
        assert_eq!(decision.placement.target_tier, StorageTier::HotStore);
        assert_eq!(
            decision.placement.pipeline_stage,
            PipelineStage::AppendHotStore
        );
        assert_eq!(decision.path_budget.path_class, IoPathClass::HotPathNvmeSsd);
        assert!(!decision.gpu_enabled);
        assert!(decision.placement.mutation_allowed);
    }
}

#[test]
fn wal_append_and_recovery_reject_cold_hdd_and_gpu_critical_path() {
    let cold_archive = OperationalProfile::cold_archive();
    cold_archive.validate().unwrap();
    let analytics = OperationalProfile::analytics_off_critical_path();
    analytics.validate().unwrap();
    let policy = CoreIoPlacementPolicy::new(analytics.hardware, analytics.workflow.thresholds);
    let cold_budget = cold_archive.workflow.segment_budget.path_budget;
    let hot_budget = analytics.workflow.page_budget.path_budget;

    for workload in [
        StorageWorkloadClass::WalAppend,
        StorageWorkloadClass::Recovery,
    ] {
        let cold_error = policy
            .plan(CoreIoPlacementRequest::new(
                workload,
                StorageIoBudgetScope::Page(PageSize::KiB16),
                cold_budget,
                false,
            ))
            .unwrap_err();
        assert_eq!(cold_error.kind(), AndromedaErrorKind::Storage);

        let gpu_error = policy
            .plan(CoreIoPlacementRequest::new(
                workload,
                StorageIoBudgetScope::Page(PageSize::KiB16),
                hot_budget,
                true,
            ))
            .unwrap_err();
        assert_eq!(gpu_error.kind(), AndromedaErrorKind::Resource);
    }
}

#[test]
fn coldstore_publication_remains_read_only_after_recovery() {
    let committed = TransactionId::new(151);
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, Some(committed.get()), b""),
        record(
            WalRecordKind::PageFormat,
            2,
            Some(1),
            Some(committed.get()),
            b"page-format",
        ),
        record(
            WalRecordKind::TxCommit,
            3,
            Some(2),
            Some(committed.get()),
            b"",
        ),
    ];

    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
    )
    .unwrap();
    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);

    let sealed = segment(SegmentState::Sealed, Lsn::new(2));
    let publication = PlacementDecision::publish_cold_segment(&sealed).unwrap();
    assert_eq!(publication.target_tier, StorageTier::ColdStore);
    assert_eq!(publication.pipeline_stage, PipelineStage::PublishColdStore);
    assert!(!publication.mutation_allowed);
    assert!(publication.validate().is_ok());

    let published = segment(SegmentState::PublishedCold, Lsn::new(2));
    let cold_segment = PublishedColdSegment::new(published).unwrap();
    for mutation in [
        SegmentMutation::AppendExtent,
        SegmentMutation::UpdatePageInPlace,
        SegmentMutation::SplitSegment,
    ] {
        assert!(published.validate_mutation(mutation).is_err());
        assert!(cold_segment.reject_mutation(mutation).is_err());
    }
}
