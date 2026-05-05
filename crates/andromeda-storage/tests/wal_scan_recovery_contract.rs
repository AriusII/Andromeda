use andromeda_core::{AndromedaErrorKind, PipelineClass, TransactionId};
use andromeda_observe::{
    CriticalDecisionKind, EventCorrelation, EventEnvelope, EventId, TraceEvent, TraceId,
};
use andromeda_storage::publication::DatabaseManifest;
use andromeda_storage::write_ahead_log::codec::{
    WalScanStopReason, encode_wal_record, scan_wal_records, scan_wal_records_from,
};
use andromeda_storage::write_ahead_log::record::{WalRecord, WalRecordKind};
use andromeda_storage::{
    AllocationId, CoreIoPlacementPolicy, CoreIoPlacementRequest, DurableTransactionState, ExtentId,
    InMemoryWal, IoPathClass, IoUseClass, Lsn, ObjectId, OperationalProfile, PageId, PageSize,
    PipelineStage, PlacementDecision, PublishedColdSegment, RecoveryPlan, RedoRecordDecision,
    SegmentDescriptor, SegmentHeader, SegmentId, SegmentMutation, SegmentState, SegmentTrailer,
    StartupMode, StorageIoBudgetScope, StorageTier, StorageWorkloadClass,
    classify_durable_transactions,
};

fn tx_record(lsn: u64, previous_lsn: Option<u64>, payload: &[u8]) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        Some(TransactionId::new(42)),
        payload,
    )
    .unwrap()
}

fn record(
    kind: WalRecordKind,
    lsn: u64,
    previous_lsn: Option<u64>,
    transaction_id: Option<u64>,
    payload: &[u8],
) -> WalRecord {
    WalRecord::from_parts(
        kind,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        transaction_id.map(TransactionId::new),
        payload,
    )
    .unwrap()
}

fn encode_records(records: &[WalRecord]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for record in records {
        encoded.extend(encode_wal_record(record).unwrap());
    }
    encoded
}

fn manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: required_wal_start_lsn,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 7,
    }
}

fn segment(state: SegmentState, max_page_lsn: Lsn) -> SegmentDescriptor {
    let header = SegmentHeader {
        magic: SegmentHeader::MAGIC,
        format_version: SegmentHeader::FORMAT_VERSION_V0,
        segment_id: SegmentId::new(301),
        object_id: ObjectId::new(302),
        allocation_id: AllocationId::new(303),
        first_page_id: PageId::new(10_000),
        page_count: 16,
        min_page_lsn: Lsn::new(1),
        max_page_lsn,
        header_crc: 304,
    };

    SegmentDescriptor {
        segment_id: header.segment_id,
        object_id: header.object_id,
        allocation_id: header.allocation_id,
        first_extent_id: ExtentId::new(305),
        extent_count: 2,
        first_page_id: header.first_page_id,
        page_count: header.page_count,
        page_size: PageSize::KiB16,
        min_page_lsn: header.min_page_lsn,
        max_page_lsn: header.max_page_lsn,
        snapshot_id: match state {
            SegmentState::BuildingHotSnapshot => None,
            SegmentState::Sealed | SegmentState::PublishedCold => Some(306),
        },
        state,
        header,
        trailer: SegmentTrailer {
            segment_payload_crc64: 307,
            segment_hash: [8; 32],
            trailer_crc: 308,
        },
    }
}

#[test]
fn recovery_replays_only_committed_transaction_records() {
    let committed = 101;
    let rolled_back = 102;
    let incomplete = 103;
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, Some(committed), b""),
        record(
            WalRecordKind::RowInsert,
            2,
            Some(1),
            Some(committed),
            b"commit-row",
        ),
        record(WalRecordKind::TxCommit, 3, Some(2), Some(committed), b""),
        record(WalRecordKind::TxBegin, 4, Some(3), Some(rolled_back), b""),
        record(
            WalRecordKind::RowInsert,
            5,
            Some(4),
            Some(rolled_back),
            b"rollback-row",
        ),
        record(
            WalRecordKind::TxRollback,
            6,
            Some(5),
            Some(rolled_back),
            b"",
        ),
        record(WalRecordKind::TxBegin, 7, Some(6), Some(incomplete), b""),
        record(
            WalRecordKind::RowInsert,
            8,
            Some(7),
            Some(incomplete),
            b"incomplete-row",
        ),
    ];

    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
    )
    .unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
    assert_eq!(
        plan.committed_replay_lsns().collect::<Vec<_>>(),
        vec![Lsn::new(2)]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(2))
            .unwrap()
            .transaction_state,
        Some(DurableTransactionState::Committed)
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(5))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipRolledBackTransaction
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(8))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn committed_redo_records_exclude_incomplete_and_rollback_terminals() {
    let committed = 111;
    let rolled_back = 112;
    let incomplete = 113;
    let records = vec![
        record(WalRecordKind::PageAllocate, 1, None, None, b"page"),
        record(WalRecordKind::TxBegin, 2, Some(1), Some(committed), b""),
        record(
            WalRecordKind::RowInsert,
            3,
            Some(2),
            Some(committed),
            b"committed-row",
        ),
        record(WalRecordKind::TxCommit, 4, Some(3), Some(committed), b""),
        record(WalRecordKind::TxBegin, 5, Some(4), Some(rolled_back), b""),
        record(
            WalRecordKind::RowUpdate,
            6,
            Some(5),
            Some(rolled_back),
            b"rolled-back-row",
        ),
        record(
            WalRecordKind::TxRollback,
            7,
            Some(6),
            Some(rolled_back),
            b"",
        ),
        record(WalRecordKind::TxBegin, 8, Some(7), Some(incomplete), b""),
        record(
            WalRecordKind::RowDelete,
            9,
            Some(8),
            Some(incomplete),
            b"incomplete-row",
        ),
    ];

    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
    )
    .unwrap();

    assert_eq!(
        plan.committed_redo_records()
            .map(|record| (record.lsn, record.transaction_state))
            .collect::<Vec<_>>(),
        vec![
            (Lsn::new(1), None),
            (Lsn::new(3), Some(DurableTransactionState::Committed)),
        ]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(6))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipRolledBackTransaction
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(9))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn recovery_continuity_replays_only_committed_inventory_update() {
    let committed = TransactionId::new(121);
    let rolled_back = TransactionId::new(122);
    let incomplete = TransactionId::new(123);
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, Some(committed.get()), b""),
        record(
            WalRecordKind::RowUpdate,
            2,
            Some(1),
            Some(committed.get()),
            b"Inventory:item=sku-7,delta=-1",
        ),
        record(
            WalRecordKind::TxCommit,
            3,
            Some(2),
            Some(committed.get()),
            b"",
        ),
        record(
            WalRecordKind::TxBegin,
            4,
            Some(3),
            Some(rolled_back.get()),
            b"",
        ),
        record(
            WalRecordKind::RowUpdate,
            5,
            Some(4),
            Some(rolled_back.get()),
            b"Inventory:item=sku-8,delta=-1",
        ),
        record(
            WalRecordKind::TxRollback,
            6,
            Some(5),
            Some(rolled_back.get()),
            b"",
        ),
        record(
            WalRecordKind::TxBegin,
            7,
            Some(6),
            Some(incomplete.get()),
            b"",
        ),
        record(
            WalRecordKind::RowUpdate,
            8,
            Some(7),
            Some(incomplete.get()),
            b"Inventory:item=sku-9,delta=-1",
        ),
    ];

    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
    )
    .unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
    assert_eq!(
        plan.committed_redo_records()
            .map(|record| (record.lsn, record.kind, record.transaction_id))
            .collect::<Vec<_>>(),
        vec![(Lsn::new(2), WalRecordKind::RowUpdate, Some(committed))]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(5))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipRolledBackTransaction
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(8))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );

    let classifications = classify_durable_transactions(records.iter());
    assert_eq!(
        classifications
            .committed_transaction_ids()
            .collect::<Vec<_>>(),
        vec![committed]
    );
    assert_eq!(
        classifications
            .rolled_back_transaction_ids()
            .collect::<Vec<_>>(),
        vec![rolled_back]
    );
    assert_eq!(
        classifications
            .incomplete_transaction_ids()
            .collect::<Vec<_>>(),
        vec![incomplete]
    );
}

#[test]
fn recovery_continuity_respects_in_memory_flush_boundary() {
    let committed = TransactionId::new(131);
    let rollback_tail = TransactionId::new(132);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(committed).unwrap();
    let committed_update_lsn = wal
        .append_payload(
            WalRecordKind::RowUpdate,
            Some(committed),
            b"Inventory:item=sku-10,delta=-1",
        )
        .unwrap();
    wal.append_tx_commit(committed).unwrap();
    wal.append_tx_begin(rollback_tail).unwrap();
    let rollback_row_lsn = wal
        .append_payload(
            WalRecordKind::RowUpdate,
            Some(rollback_tail),
            b"Inventory:item=sku-11,delta=-1",
        )
        .unwrap();
    let unflushed_rollback_terminal = wal.append_tx_rollback(rollback_tail).unwrap();

    wal.flush_through(rollback_row_lsn).unwrap();
    assert_eq!(wal.durable_lsn(), rollback_row_lsn);
    assert!(unflushed_rollback_terminal > wal.durable_lsn());

    let durable_records = wal.replay_durable();
    assert_eq!(
        durable_records.last().map(|record| record.header.lsn),
        Some(rollback_row_lsn)
    );
    assert!(
        durable_records
            .iter()
            .all(|record| record.header.lsn < unflushed_rollback_terminal)
    );

    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &durable_records,
    )
    .unwrap();

    assert_eq!(
        plan.replay_lsns().collect::<Vec<_>>(),
        vec![committed_update_lsn]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == rollback_row_lsn)
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );

    let classifications = wal.classify_durable_transactions();
    assert_eq!(
        classifications
            .committed_transaction_ids()
            .collect::<Vec<_>>(),
        vec![committed]
    );
    assert_eq!(
        classifications
            .rolled_back_transaction_ids()
            .collect::<Vec<_>>(),
        Vec::<TransactionId>::new()
    );
    assert_eq!(
        classifications
            .incomplete_transaction_ids()
            .collect::<Vec<_>>(),
        vec![rollback_tail]
    );
}

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

#[test]
fn recovery_requires_manifest_start_to_be_covered_and_anchored() {
    let missing_start = vec![record(
        WalRecordKind::PageAllocate,
        6,
        Some(5),
        None,
        b"page",
    )];
    assert!(
        RecoveryPlan::from_manifest_and_wal(
            &manifest(Lsn::new(5)),
            StartupMode::SafeStart,
            &missing_start,
        )
        .is_err()
    );

    let unanchored_start = vec![record(WalRecordKind::PageAllocate, 5, None, None, b"page")];
    assert!(
        RecoveryPlan::from_manifest_and_wal(
            &manifest(Lsn::new(5)),
            StartupMode::SafeStart,
            &unanchored_start,
        )
        .is_err()
    );

    let anchored_start = vec![record(
        WalRecordKind::PageAllocate,
        5,
        Some(4),
        None,
        b"page",
    )];
    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(5)),
        StartupMode::SafeStart,
        &anchored_start,
    )
    .unwrap();
    assert_eq!(plan.coverage.first_replay_record_lsn, Some(Lsn::new(5)));
    assert_eq!(plan.coverage.record_count_in_redo_range, 1);
}

#[test]
fn recovery_from_scan_keeps_valid_prefix_and_records_tail_boundary() {
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, Some(201), b""),
        record(WalRecordKind::RowUpdate, 2, Some(1), Some(201), b"durable"),
        record(WalRecordKind::TxCommit, 3, Some(2), Some(201), b""),
        record(WalRecordKind::RowInsert, 4, Some(3), Some(202), b"tail"),
    ];
    let mut encoded = encode_records(&records);
    encoded.truncate(encoded.len() - 1);

    let scan = scan_wal_records(&encoded);
    assert_eq!(scan.records.len(), 3);
    assert_eq!(
        scan.stopped.unwrap().reason,
        WalScanStopReason::TruncatedRecord
    );

    let plan = RecoveryPlan::from_manifest_and_wal_scan(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &scan,
    )
    .unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
    assert_eq!(
        plan.wal_scan_stop().unwrap().reason,
        WalScanStopReason::TruncatedRecord
    );
}

#[test]
fn recovery_trace_marks_recoverable_wal_tail_stop_as_boundary_evidence() {
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, Some(211), b""),
        record(WalRecordKind::RowUpdate, 2, Some(1), Some(211), b"durable"),
        record(WalRecordKind::TxCommit, 3, Some(2), Some(211), b""),
        record(WalRecordKind::RowInsert, 4, Some(3), Some(212), b"tail"),
    ];
    let mut encoded = encode_records(&records);
    encoded.truncate(encoded.len() - 1);
    let scan = scan_wal_records(&encoded);

    let plan = RecoveryPlan::from_manifest_and_wal_scan(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &scan,
    )
    .expect("recoverable WAL tail truncation should retain durable prefix for recovery");
    let trace = plan.observe_recovery_trace(TraceId::new(601));

    assert!(trace.proves_recovery_boundary());
    assert_eq!(trace.last_durable_lsn, plan.durable_lsn.get());
    assert_eq!(trace.corruption_boundary_lsn, Some(plan.durable_lsn.get()));

    let envelope = EventEnvelope::new(
        EventId::new(602),
        EventCorrelation {
            durable_lsn: Some(plan.durable_lsn.get()),
            ..EventCorrelation::empty()
        },
        TraceEvent::RecoveryStartup(trace),
    )
    .expect("recovery startup trace must carry durable WAL boundary evidence");

    assert_eq!(envelope.event.kind(), CriticalDecisionKind::RecoveryStartup);
    assert_eq!(
        envelope.correlation.durable_lsn,
        Some(plan.durable_lsn.get())
    );
}

#[test]
fn recovery_from_scan_rejects_non_recoverable_chain_boundaries() {
    let records = vec![tx_record(1, None, b"first"), tx_record(3, Some(1), b"gap")];
    let scan = scan_wal_records(&encode_records(&records));

    assert_eq!(scan.stopped.unwrap().reason, WalScanStopReason::LsnGap);
    assert!(
        RecoveryPlan::from_manifest_and_wal_scan(
            &manifest(Lsn::new(1)),
            StartupMode::SafeStart,
            &scan,
        )
        .is_err()
    );
}

#[test]
fn wal_scan_from_manifest_boundary_accepts_expected_previous_lsn() {
    let records = vec![
        record(WalRecordKind::PageAllocate, 5, Some(4), None, b"page"),
        record(WalRecordKind::PageFormat, 6, Some(5), None, b"format"),
    ];

    let scan = scan_wal_records_from(&encode_records(&records), Lsn::new(5), Some(Lsn::new(4)));

    assert!(scan.is_complete());
    assert_eq!(scan.records, records);
}

#[test]
fn wal_scan_keeps_valid_prefix_for_truncated_and_corrupt_tail() {
    let records = vec![
        tx_record(1, None, b"first"),
        tx_record(2, Some(1), b"second"),
    ];
    let encoded = encode_records(&records);

    let mut truncated = encoded.clone();
    truncated.truncate(truncated.len() - 1);
    let truncated_scan = scan_wal_records(&truncated);
    assert_eq!(truncated_scan.records, vec![records[0].clone()]);
    assert_eq!(
        truncated_scan.valid_bytes,
        encode_records(&records[..1]).len()
    );
    assert_eq!(
        truncated_scan.stopped.unwrap().reason,
        WalScanStopReason::TruncatedRecord
    );

    let mut corrupt = encoded;
    let last_payload_byte = corrupt.len() - 1;
    corrupt[last_payload_byte] ^= 0x5a;
    let corrupt_scan = scan_wal_records(&corrupt);
    assert_eq!(corrupt_scan.records, vec![records[0].clone()]);
    assert_eq!(
        corrupt_scan.stopped.unwrap().reason,
        WalScanStopReason::CorruptRecord
    );
}

#[test]
fn wal_scan_and_recovery_reject_lsn_gap() {
    let records = vec![tx_record(1, None, b"first"), tx_record(3, Some(1), b"gap")];

    let scan = scan_wal_records(&encode_records(&records));
    assert_eq!(scan.records, vec![records[0].clone()]);
    assert_eq!(scan.stopped.unwrap().reason, WalScanStopReason::LsnGap);

    let recovery = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
    );
    assert!(recovery.is_err());
}

#[test]
fn wal_scan_rejects_previous_lsn_chain_mismatch() {
    let records = vec![
        tx_record(1, None, b"first"),
        tx_record(2, None, b"wrong-previous"),
    ];

    let scan = scan_wal_records(&encode_records(&records));

    assert_eq!(scan.records, vec![records[0].clone()]);
    assert_eq!(
        scan.stopped.unwrap().reason,
        WalScanStopReason::PreviousLsnMismatch
    );
}
