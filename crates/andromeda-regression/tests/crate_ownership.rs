#![forbid(unsafe_code)]

use std::{collections::BTreeSet, fs, path::PathBuf};

#[test]
fn crate_ownership_migrated_observability_consumers_import_trace_contracts_from_owner() {
    let workspace = workspace_root();
    let migrated_roots = [
        "crates/andromeda-plan-cache",
        "crates/andromeda-procedure-store",
        "crates/andromeda-restore",
        "crates/andromeda-retry",
        "crates/andromeda-scenario-evidence",
        "crates/andromeda-backup",
        "crates/andromeda-inventory-demo-cli-adapter",
    ];

    let violations = migrated_roots
        .iter()
        .flat_map(|root| rust_source_files(&workspace.join(root)))
        .flat_map(|file| forbidden_token_violations(&workspace, &file, "andromeda_observe::"))
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "migrated observability consumers must import TraceId/DecisionTrace contracts from andromeda-observability instead of the observe facade:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_contract_consumers_import_procedure_contract_owner() {
    let workspace = workspace_root();
    let migrated_roots = [
        "crates/andromeda-admission",
        "crates/andromeda-decision-trace",
        "crates/andromeda-plan-cache",
        "crates/andromeda-statistics",
        "crates/andromeda-procedure-runtime",
    ];

    let mut violations = Vec::new();
    for root in migrated_roots {
        let root_path = workspace.join(root);
        for file in rust_source_files(&root_path) {
            violations.extend(forbidden_token_violations(
                &workspace,
                &file,
                "andromeda_contract::",
            ));
        }

        let manifest = root_path.join("Cargo.toml");
        let manifest_source = fs::read_to_string(&manifest)
            .unwrap_or_else(|err| panic!("read {}: {err}", manifest.display()));
        if manifest_source.contains("andromeda-contract.workspace") {
            violations.push(format!(
                "{root}/Cargo.toml depends on `andromeda-contract`; use `andromeda-procedure-contract`"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "migrated contract consumers must import ProcedureContract/PolicyVersion/StatsVersion from andromeda-procedure-contract instead of the contract facade:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_migrated_fuzz_targets_import_storage_and_catalog_recovery_owners() {
    let workspace = workspace_root();
    let migrated_files = [
        "fuzz/fuzz_targets/definition_batch.rs",
        "fuzz/fuzz_targets/heap_page_v1_decode.rs",
        "fuzz/fuzz_targets/manifest_decode.rs",
    ];
    let forbidden = [
        (
            "andromeda_catalog::CatalogMutationRecord",
            "andromeda_catalog_recovery::CatalogMutationRecord",
        ),
        (
            "andromeda_storage::HeapPage",
            "andromeda_storage_heap::HeapPage",
        ),
        (
            "andromeda_storage::ReplayContext",
            "andromeda_recovery::ReplayContext",
        ),
        (
            "andromeda_storage::replay_wal_record",
            "andromeda_recovery::replay_wal_record",
        ),
    ];

    let mut violations = Vec::new();
    for relative in migrated_files {
        let file = workspace.join(relative);
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
        let code = strip_rust_comments(&source);
        for (bad, good) in forbidden {
            if code.contains(bad) {
                violations.push(format!("{relative} imports `{bad}`; use `{good}`"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "migrated fuzz targets must import canonical owner crates:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_historical_facades_do_not_use_glob_reexports() {
    let workspace = workspace_root();
    let historical_facades = [
        "crates/andromeda-contract/src/lib.rs",
        "crates/andromeda-proto/src/lib.rs",
        "crates/andromeda-srpl/src/lib.rs",
        "crates/andromeda-bench/src/lib.rs",
    ];

    let mut violations = Vec::new();
    for relative in historical_facades {
        let file = workspace.join(relative);
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
        let code = strip_rust_comments(&source);
        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("pub use ") && trimmed.ends_with("::*;") {
                violations.push(format!(
                    "{relative} uses `{trimmed}`; export an explicit compatibility surface"
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "historical root facades must not grow via glob reexports:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_proto_facade_is_schema_only_for_migrated_callers() {
    let workspace = workspace_root();
    let proto_lib = workspace.join("crates/andromeda-proto/src/lib.rs");
    let proto_source = fs::read_to_string(&proto_lib)
        .unwrap_or_else(|err| panic!("read {}: {err}", proto_lib.display()));
    let proto_code = strip_rust_comments(&proto_source);
    let forbidden_reexports = [
        "pub use andromeda_procedure_contract",
        "pub use andromeda_proto_wire",
        "pub use andromeda_structured_object",
        "pub use generated::{\n    decode_generated_message",
        "pub use generated::{\n    project_generated_",
        "pub use generated::{\n    validate_",
    ];
    let mut violations = forbidden_reexports
        .iter()
        .filter(|bad| proto_code.contains(**bad))
        .map(|bad| {
            format!(
                "crates/andromeda-proto/src/lib.rs reexports `{bad}`; keep andromeda-proto schema/generated only"
            )
        })
        .collect::<Vec<_>>();

    let migrated_files = [
        "crates/andromeda-admission/src/invocation.rs",
        "crates/andromeda-rpc-codec/src/invocation_response.rs",
        "crates/andromeda-exec/tests/v0_transition_lifecycle.rs",
        "fuzz/fuzz_targets/contract_hash.rs",
        "fuzz/fuzz_targets/proto_frame_envelope_decode.rs",
        "fuzz/fuzz_targets/proto_rpc_completion_decode.rs",
    ];
    let forbidden = [
        "andromeda_proto::StructuredObjectHeader",
        "andromeda_proto::StructuredObjectLayout",
        "andromeda_proto::RowCountPolicy",
        "andromeda_proto::RpcCompletionStatus",
        "andromeda_proto::ProtocolVersion",
        "andromeda_proto::PayloadKind",
    ];

    for relative in migrated_files {
        let file = workspace.join(relative);
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
        let code = strip_rust_comments(&source);
        for bad in forbidden {
            if code.contains(bad) {
                violations.push(format!(
                    "{relative} imports `{bad}`; use the owner crate instead"
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "migrated callers must use andromeda-proto only for generated schema access:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_storage_facade_does_not_reintroduce_pure_compat_wrappers() {
    let workspace = workspace_root();
    let storage_lib = workspace.join("crates/andromeda-storage/src/lib.rs");
    let source = fs::read_to_string(&storage_lib)
        .unwrap_or_else(|err| panic!("read {}: {err}", storage_lib.display()));
    let code = strip_rust_comments(&source);
    let forbidden = [
        (
            "mod recovery;",
            "andromeda_recovery / andromeda_catalog_recovery APIs",
        ),
        (
            "pub mod write_ahead_log;",
            "andromeda_wal / andromeda_recovery / andromeda_manifest APIs",
        ),
        (
            "pub use recovery",
            "andromeda_recovery / andromeda_catalog_recovery APIs",
        ),
        (
            "pub use write_ahead_log",
            "andromeda_wal / andromeda_recovery / andromeda_manifest APIs",
        ),
        (
            "mod cold_store;",
            "andromeda_manifest::PublishedColdSegment",
        ),
        ("mod file_wal;", "andromeda_recovery file WAL APIs"),
        (
            "mod wal_record_catalog;",
            "andromeda_catalog_recovery storage WAL APIs",
        ),
        ("mod btree_key_codec;", "andromeda_storage_index key APIs"),
        (
            "mod btree_format_validation;",
            "andromeda_storage_index format validation APIs",
        ),
        (
            "pub mod publication;",
            "andromeda_manifest publication APIs",
        ),
        ("mod manifest;", "andromeda_manifest manifest APIs"),
        (
            "pub use cold_store",
            "andromeda_manifest::PublishedColdSegment",
        ),
        ("pub use file_wal", "andromeda_recovery file WAL APIs"),
        (
            "pub use wal_record_catalog",
            "andromeda_catalog_recovery storage WAL APIs",
        ),
        (
            "pub use btree_key_codec",
            "andromeda_storage_index key APIs",
        ),
        (
            "pub use btree_format_validation",
            "andromeda_storage_index format validation APIs",
        ),
        ("pub use manifest", "andromeda_manifest manifest APIs"),
    ];
    let demolished_files = [(
        "crates/andromeda-storage/src/heap/mod.rs",
        "andromeda-storage-heap heap APIs",
    )];

    let mut violations = forbidden
        .iter()
        .filter_map(|(bad, good)| {
            code.contains(bad)
                .then(|| format!("andromeda-storage lib.rs exposes `{bad}`; use `{good}`"))
        })
        .collect::<Vec<_>>();
    violations.extend(demolished_files.iter().filter_map(|(relative, owner)| {
        workspace
            .join(relative)
            .exists()
            .then(|| format!("{relative} reintroduced a storage facade file; use `{owner}`"))
    }));

    assert!(
        violations.is_empty(),
        "andromeda-storage must not reintroduce pure compatibility wrapper modules:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_storage_facade_does_not_reexport_owner_crate_surfaces() {
    let workspace = workspace_root();
    let storage_lib = workspace.join("crates/andromeda-storage/src/lib.rs");
    let source = fs::read_to_string(&storage_lib)
        .unwrap_or_else(|err| panic!("read {}: {err}", storage_lib.display()));
    let code = strip_rust_comments(&source);
    let forbidden_pub_use_prefixes = [
        ("pub use andromeda_buffer_pool", "andromeda-buffer-pool"),
        (
            "pub use andromeda_disk_page_store",
            "andromeda-disk-page-store",
        ),
        ("pub use andromeda_segment", "andromeda-segment"),
        ("pub use andromeda_storage_heap", "andromeda-storage-heap"),
        ("pub use andromeda_storage_index", "andromeda-storage-index"),
        ("pub use andromeda_storage_page", "andromeda-storage-page"),
        ("pub use heap", "andromeda-storage-heap"),
    ];
    let forbidden_root_items = [
        (
            "DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS",
            "andromeda_manifest::DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS",
        ),
        ("DatabaseManifest", "andromeda_manifest::DatabaseManifest"),
        (
            "DatabaseSnapshotPublication",
            "andromeda_manifest::DatabaseSnapshotPublication",
        ),
        (
            "ManifestDurabilityBoundary",
            "andromeda_manifest::ManifestDurabilityBoundary",
        ),
        (
            "SnapshotAvailabilityContract",
            "andromeda_manifest::SnapshotAvailabilityContract",
        ),
        (
            "SnapshotSegmentReference",
            "andromeda_manifest::SnapshotSegmentReference",
        ),
        (
            "StorageFormatManifest",
            "andromeda_manifest::StorageFormatManifest",
        ),
        ("HeapPage", "andromeda_storage_heap::HeapPage"),
        ("ReplayContext", "andromeda_recovery::ReplayContext"),
        ("replay_wal_record", "andromeda_recovery::replay_wal_record"),
    ];

    let mut violations = Vec::new();
    for (bad, owner) in forbidden_pub_use_prefixes {
        if code.contains(bad) {
            violations.push(format!(
                "andromeda-storage lib.rs exposes `{bad}`; import from `{owner}`"
            ));
        }
    }

    let reexports = facade_reexport_items(&source);
    for (bad, owner_path) in forbidden_root_items {
        if reexports.contains(bad) {
            violations.push(format!(
                "andromeda-storage root reexports `{bad}`; import `{owner_path}`"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "andromeda-storage must not reintroduce owner-crate compatibility reexports without active facade consumers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_transaction_tests_import_log_and_mvcc_owners() {
    let workspace = workspace_root();
    let migrated_files = [
        "crates/andromeda-transaction/tests/commit_log_durability.rs",
        "crates/andromeda-transaction/tests/manager_invariants.rs",
        "crates/andromeda-transaction/tests/storage_tx_wal_adapter_contract.rs",
        "crates/andromeda-transaction/tests/v0_transaction_lifecycle/state_machine_durability.rs",
        "crates/andromeda-transaction/tests/v0_transaction_lifecycle/manager_visibility.rs",
        "crates/andromeda-transaction/tests/v0_transition_lifecycle.rs",
    ];
    let forbidden_transaction_items = [
        "CommitLogEntry",
        "RollbackLogEntry",
        "CommitLogManager",
        "Lsn",
        "WalRecordKind",
        "TxWalAdapterError",
        "TxWalAdapterReplayKind",
        "TxWalAdapterReplayRecord",
        "TxWalReplayAction",
        "TxWalReplayRecord",
        "TxWalReplaySummary",
        "map_tx_wal_replay_records",
        "MvccIsolationPolicy",
        "MvccRowHeader",
        "Snapshot",
        "TransactionStatus",
        "TransactionStatusTable",
    ];

    let mut violations = Vec::new();
    for relative in migrated_files {
        let file = workspace.join(relative);
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
        let code = strip_rust_comments(&source);

        for item in forbidden_transaction_items {
            let direct_import = format!("andromeda_transaction::{item}");
            if code.contains(&direct_import) {
                violations.push(format!(
                    "{relative} imports `{direct_import}`; use the owner crate"
                ));
            }
        }

        violations.extend(forbidden_grouped_import_items(
            relative,
            &code,
            "andromeda_transaction",
            &forbidden_transaction_items,
        ));
    }

    assert!(
        violations.is_empty(),
        "transaction tests must import transaction-log and MVCC owned contracts from their owner crates:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_recovery_tests_import_observability_ids_from_owner() {
    let workspace = workspace_root();
    let recovery_test_files = [
        "crates/andromeda-recovery/tests/wal_scan_recovery_contract/scan_boundaries.rs",
        "crates/andromeda-recovery/tests/recovery_planning_contract/scan_trace.rs",
    ];
    let forbidden_observe_imports = [
        "CriticalDecisionKind",
        "EventCorrelation",
        "EventId",
        "TraceId",
    ];

    let mut violations = Vec::new();
    for relative in recovery_test_files {
        let file = workspace.join(relative);
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
        let code = strip_rust_comments(&source);
        violations.extend(forbidden_grouped_import_items(
            relative,
            &code,
            "andromeda_observe",
            &forbidden_observe_imports,
        ));
    }

    assert!(
        violations.is_empty(),
        "recovery tests must import observability IDs and decision kinds from andromeda-observability while keeping observe event routing on andromeda-observe:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_migrated_files_import_observability_and_catalog_store_owners() {
    let workspace = workspace_root();
    let file_checks = [
        (
            "crates/andromeda-admission/src/context.rs",
            "andromeda_observe::EventCorrelation",
            "andromeda_observability::EventCorrelation",
        ),
        (
            "crates/andromeda-admission/src/context.rs",
            "andromeda_observe::EventId",
            "andromeda_observability::EventId",
        ),
        (
            "crates/andromeda-admission/src/invocation.rs",
            "andromeda_observe::EventCorrelation",
            "andromeda_observability::EventCorrelation",
        ),
        (
            "crates/andromeda-admission/src/invocation.rs",
            "andromeda_observe::EventId",
            "andromeda_observability::EventId",
        ),
        (
            "crates/andromeda-exec/src/local/helpers.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-exec/src/local/runtime.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-exec/src/local/runtime.rs",
            "andromeda_observe::EventCorrelation",
            "andromeda_observability::EventCorrelation",
        ),
        (
            "crates/andromeda-exec/src/local/runtime/events.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-exec/src/local/runtime/events.rs",
            "andromeda_observe::EventCorrelation",
            "andromeda_observability::EventCorrelation",
        ),
        (
            "crates/andromeda-recovery/src/transaction_wal_bridge.rs",
            "andromeda_transaction::TxWalAdapterReplayRecord",
            "andromeda_transaction_log::TxWalAdapterReplayRecord",
        ),
        (
            "crates/andromeda-recovery/src/transaction_wal_bridge.rs",
            "andromeda_transaction::map_tx_wal_replay_records",
            "andromeda_transaction_log::map_tx_wal_replay_records",
        ),
        (
            "crates/andromeda-srpl-catalog-binding/src/lib.rs",
            "andromeda_catalog::CatalogSnapshot",
            "andromeda_catalog_store::CatalogSnapshot",
        ),
        (
            "crates/andromeda-exec/src/local/runtime.rs",
            "andromeda_catalog::CatalogSnapshot",
            "andromeda_catalog_store::CatalogSnapshot",
        ),
        (
            "crates/andromeda-exec/src/local/runtime/admission.rs",
            "andromeda_catalog::CatalogSnapshot",
            "andromeda_catalog_store::CatalogSnapshot",
        ),
        (
            "crates/andromeda-exec/src/local/runtime.rs",
            "andromeda_catalog::CatalogPublicationReceipt",
            "andromeda_catalog_store::CatalogPublicationReceipt",
        ),
        (
            "crates/andromeda-exec/src/local/runtime/admission.rs",
            "andromeda_catalog::CatalogPublicationReceipt",
            "andromeda_catalog_store::CatalogPublicationReceipt",
        ),
        (
            "crates/andromeda-procedure-runtime/src/dispatch.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-procedure-runtime/src/dispatch.rs",
            "andromeda_observe::DecisionTrace",
            "andromeda_observability::DecisionTrace",
        ),
        (
            "crates/andromeda-procedure-runtime/src/dispatch.rs",
            "andromeda_observe::CriticalDecisionKind",
            "andromeda_observability::CriticalDecisionKind",
        ),
        (
            "crates/andromeda-execution/src/procedure_registry.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-execution/src/procedure_registry.rs",
            "andromeda_observe::DecisionTrace",
            "andromeda_observability::DecisionTrace",
        ),
        (
            "crates/andromeda-exec/src/local/types.rs",
            "andromeda_observe::DecisionTrace",
            "andromeda_observability::DecisionTrace",
        ),
        (
            "crates/andromeda-exec/src/dispatch/permission_validation.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-cli/src/audit/mod.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-inventory-demo/src/business/types.rs",
            "andromeda_observe::DecisionTrace",
            "andromeda_observability::DecisionTrace",
        ),
        (
            "crates/andromeda-inventory-demo/src/business/types.rs",
            "andromeda_observe::CriticalDecisionKind",
            "andromeda_observability::CriticalDecisionKind",
        ),
        (
            "crates/andromeda-inventory-demo/src/business/types.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-inventory-demo/src/business/executor.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-inventory-demo/src/vertical_slice_entry.rs",
            "andromeda_catalog::CatalogSnapshot",
            "andromeda_catalog_store::CatalogSnapshot",
        ),
        (
            "crates/andromeda-inventory-demo/src/vertical_slice_entry.rs",
            "andromeda_catalog::CatalogPublicationReceipt",
            "andromeda_catalog_store::CatalogPublicationReceipt",
        ),
        (
            "crates/andromeda-inventory-demo-cli-adapter/src/lib.rs",
            "andromeda_catalog::CatalogSnapshot",
            "andromeda_catalog_store::CatalogSnapshot",
        ),
        (
            "crates/andromeda-inventory-demo-cli-adapter/src/lib.rs",
            "andromeda_catalog::CatalogPublicationReceipt",
            "andromeda_catalog_store::CatalogPublicationReceipt",
        ),
        (
            "crates/andromeda-inventory-demo-cli-adapter/src/lib.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/support.rs",
            "andromeda_catalog::CatalogSnapshot",
            "andromeda_catalog_store::CatalogSnapshot",
        ),
        (
            "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/support.rs",
            "andromeda_catalog::CatalogPublicationReceipt",
            "andromeda_catalog_store::CatalogPublicationReceipt",
        ),
        (
            "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/support.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/cataloged_procedure_flow.rs",
            "andromeda_observe::CriticalDecisionKind",
            "andromeda_observability::CriticalDecisionKind",
        ),
        (
            "crates/andromeda-inventory-demo/tests/v0_vertical_e2e/failure_rollback_gates.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-exec/tests/runtime_contract/common.rs",
            "andromeda_observe::TraceId",
            "andromeda_observability::TraceId",
        ),
        (
            "crates/andromeda-exec/tests/integration_execution_path/support.rs",
            "andromeda_observe::EventCorrelation",
            "andromeda_observability::EventCorrelation",
        ),
        (
            "crates/andromeda-exec/tests/integration_execution_path/support.rs",
            "andromeda_observe::EventId",
            "andromeda_observability::EventId",
        ),
        (
            "crates/andromeda-exec/tests/permission_scope_contract.rs",
            "andromeda_observe::DecisionTrace",
            "andromeda_observability::DecisionTrace",
        ),
        (
            "crates/andromeda-exec/tests/executor_validation_gates/support.rs",
            "andromeda_observe::CriticalDecisionKind",
            "andromeda_observability::CriticalDecisionKind",
        ),
        (
            "crates/andromeda-observe/tests/audit_family_contract.rs",
            "andromeda_observe::EventCorrelation",
            "andromeda_observability::EventCorrelation",
        ),
        (
            "crates/andromeda-observe/tests/durable_audit_sink_contract.rs",
            "andromeda_observe::EventCorrelation",
            "andromeda_observability::EventCorrelation",
        ),
        (
            "crates/andromeda-srpl/tests/compiler_pipeline_e2e/support.rs",
            "andromeda_catalog::CatalogSnapshot",
            "andromeda_catalog_store::CatalogSnapshot",
        ),
        (
            "crates/andromeda-srpl-catalog-binding/tests/owner_direct.rs",
            "andromeda_catalog::CatalogSnapshot",
            "andromeda_catalog_store::CatalogSnapshot",
        ),
        (
            "crates/andromeda-srpl-catalog-binding/src/lib.rs",
            "andromeda_catalog::CatalogPublicationReceipt",
            "andromeda_catalog_store::CatalogPublicationReceipt",
        ),
        (
            "crates/andromeda-srpl-catalog-binding/tests/owner_direct.rs",
            "andromeda_catalog::CatalogPublicationReceipt",
            "andromeda_catalog_store::CatalogPublicationReceipt",
        ),
        (
            "crates/andromeda-storage/tests/api_compat_reexports.rs",
            "andromeda_storage::",
            "storage-family owner crates",
        ),
        (
            "crates/andromeda-storage/tests/btree_durable_promotion_contract.rs",
            "andromeda_storage::",
            "andromeda_storage_index",
        ),
        (
            "crates/andromeda-storage/tests/page_ownership_invariants.rs",
            "andromeda_storage::",
            "andromeda_storage_page",
        ),
        (
            "crates/andromeda-recovery/tests/transaction_wal_bridge.rs",
            "andromeda_transaction::CommitLogManager",
            "andromeda_transaction_log::CommitLogManager",
        ),
        (
            "crates/andromeda-transaction/tests/tx_wal_replay_recovery.rs",
            "andromeda_transaction::CommitLogManager",
            "andromeda_transaction_log::CommitLogManager",
        ),
        (
            "crates/andromeda-catalog/tests/catalog_digest_contract.rs",
            "andromeda_catalog::digest",
            "andromeda_digest",
        ),
        (
            "fuzz/fuzz_targets/durable_audit_journal_support.rs",
            "andromeda_observe::",
            "andromeda_audit / andromeda_observability",
        ),
        (
            "crates/andromeda-bench/src/audit_file_benchmark.rs",
            "PendingDurableAuditRecord",
            "andromeda_audit::DurableAuditAppendRecord",
        ),
        (
            "crates/andromeda-bench/src/audit_file_benchmark.rs",
            "andromeda_observe::FileDurableAuditWalSink",
            "andromeda_audit::FileDurableAuditWalSink",
        ),
        (
            "crates/andromeda-cli/src/audit/mod.rs",
            "andromeda_observe::",
            "andromeda_audit / andromeda_observability",
        ),
        (
            "crates/andromeda-cli/src/audit/parsing.rs",
            "andromeda_observe::",
            "andromeda_audit / andromeda_observability",
        ),
        (
            "crates/andromeda-cli/src/audit/output/human.rs",
            "andromeda_observe::",
            "andromeda_audit / andromeda_observability",
        ),
        (
            "crates/andromeda-cli/src/audit/output/json.rs",
            "andromeda_observe::",
            "andromeda_audit / andromeda_observability",
        ),
        (
            "crates/andromeda-cli/src/audit/output/labels.rs",
            "andromeda_observe::",
            "andromeda_audit / andromeda_observability",
        ),
        (
            "crates/andromeda-cli/src/audit/output/records.rs",
            "andromeda_observe::",
            "andromeda_audit / andromeda_observability",
        ),
        (
            "crates/andromeda-srpl-definition-batch/src/dry_run.rs",
            "andromeda_catalog::CatalogMutationRecordKind",
            "andromeda_catalog_recovery::CatalogMutationRecordKind",
        ),
        (
            "crates/andromeda-srpl-catalog-binding/tests/owner_direct.rs",
            "andromeda_catalog::DefinitionBatch",
            "andromeda_definition_batch::DefinitionBatch",
        ),
        (
            "crates/andromeda-hadr/src/cluster_security.rs",
            "andromeda_observe::",
            "andromeda_audit",
        ),
        (
            "crates/andromeda-hadr/tests/hadr_promotion_runtime_contract.rs",
            "andromeda_observe::",
            "andromeda_audit / andromeda_observability",
        ),
        (
            "crates/andromeda-hadr/tests/quorum_membership_contract.rs",
            "andromeda_observe::",
            "andromeda_audit / andromeda_observability",
        ),
        (
            "crates/andromeda-cli/src/hadr/promote.rs",
            "andromeda_observe::",
            "andromeda_audit / andromeda_observability",
        ),
    ];

    let mut violations = Vec::new();
    for (relative, bad, good) in file_checks {
        let file = workspace.join(relative);
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
        let code = strip_rust_comments(&source);
        if code.contains(bad) {
            violations.push(format!("{relative} imports `{bad}`; use `{good}`"));
        }
    }

    assert!(
        violations.is_empty(),
        "migrated files must import canonical owner crates:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_observe_tests_import_audit_and_observability_owners() {
    let workspace = workspace_root();
    let observe_test_files = [
        "crates/andromeda-observe/tests/audit_family_contract.rs",
        "crates/andromeda-observe/tests/durable_audit_sink_contract.rs",
        "crates/andromeda-observe/tests/io_pipeline_contract/support.rs",
        "crates/andromeda-observe/tests/placement_audit_contract.rs",
        "crates/andromeda-observe/tests/protocol_correlation_contract.rs",
        "crates/andromeda-observe/tests/v0_procedure_lifecycle.rs",
        "crates/andromeda-observe/src/query/tests.rs",
    ];
    let forbidden_observe_imports = [
        "AdminOperation",
        "AdminOperationTrace",
        "AuditTrace",
        "AuthorizationDeniedTrace",
        "BackpressureTrace",
        "CertificateIdentity",
        "CompletionEmittedTrace",
        "CriticalDecisionKind",
        "DecisionTrace",
        "EventCorrelation",
        "EventId",
        "ExecutionTransitionTrace",
        "MvccTrace",
        "Permission",
        "ProtocolCorrelation",
        "ProtocolEventScope",
        "SecurityAuditOutcome",
        "SecurityAuditTrace",
        "SecurityPolicyVersionEvidence",
        "SurfaceScope",
        "TraceEventFamily",
        "TraceId",
        "TraceQueryFilter",
        "TraceQueryLsnRange",
        "TraceQuerySpec",
        "TransitionReasonCode",
        "UserPrincipal",
        "UserPrincipalKind",
        "V0_EVENT_SCHEMA_VERSION",
    ];

    let mut violations = Vec::new();
    for relative in observe_test_files {
        let file = workspace.join(relative);
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
        let code = strip_rust_comments(&source);
        violations.extend(forbidden_grouped_import_items(
            relative,
            &code,
            "andromeda_observe",
            &forbidden_observe_imports,
        ));
    }

    assert!(
        violations.is_empty(),
        "observe tests must import audit-owned types from andromeda-audit and observability primitives from andromeda-observability, while keeping observe runtime/query surfaces on andromeda-observe:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_catalog_definition_batch_planning_is_named_integrator_only() {
    let workspace = workspace_root();
    let definition_file = workspace.join("crates/andromeda-catalog/src/batch/definition.rs");
    let source = fs::read_to_string(&definition_file)
        .unwrap_or_else(|err| panic!("read {}: {err}", definition_file.display()));
    let code = strip_rust_comments(&source);
    let forbidden_reexports = [
        "pub use andromeda_definition_batch",
        "pub(crate) use andromeda_definition_batch",
        "pub use crate::batch::definition::DefinitionBatch",
        "pub use crate::batch::definition::DefinitionOperation",
        "pub use crate::batch::definition::CatalogLifecycleTarget",
    ];
    let violations = forbidden_reexports
        .iter()
        .filter(|bad| code.contains(**bad))
        .map(|bad| {
            format!(
                "crates/andromeda-catalog/src/batch/definition.rs reexports `{bad}`; import DefinitionBatch DTOs from andromeda-definition-batch"
            )
        })
        .collect::<Vec<_>>();

    assert!(
        code.contains("pub trait CatalogDefinitionBatchPlanning"),
        "CatalogDefinitionBatchPlanning must remain explicit if it is the catalog runtime planning boundary"
    );
    assert!(
        violations.is_empty(),
        "CatalogDefinitionBatchPlanning may stay as a named catalog runtime integrator surface, but DefinitionBatch DTOs must not be reexported through the catalog facade:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_major_facades_do_not_grow_reexport_surfaces() {
    let workspace = workspace_root();
    let guards = [
        FacadeReexportGuard {
            crate_name: "andromeda-catalog",
            relative_lib_rs: "crates/andromeda-catalog/src/lib.rs",
            allowed_reexports: CATALOG_FACADE_REEXPORT_ALLOWLIST,
        },
        FacadeReexportGuard {
            crate_name: "andromeda-observe",
            relative_lib_rs: "crates/andromeda-observe/src/lib.rs",
            allowed_reexports: OBSERVE_FACADE_REEXPORT_ALLOWLIST,
        },
        FacadeReexportGuard {
            crate_name: "andromeda-transaction",
            relative_lib_rs: "crates/andromeda-transaction/src/lib.rs",
            allowed_reexports: TRANSACTION_FACADE_REEXPORT_ALLOWLIST,
        },
    ];

    let mut violations = Vec::new();
    for guard in guards {
        let file = workspace.join(guard.relative_lib_rs);
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
        let actual = facade_reexport_items(&source);
        let allowed = guard
            .allowed_reexports
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let unexpected = actual
            .iter()
            .filter(|item| !allowed.contains(item.as_str()))
            .map(String::as_str)
            .collect::<Vec<_>>();

        if !unexpected.is_empty() {
            violations.push(format!(
                "{} exposes new facade reexports not in the crate-ownership allowlist: {}",
                guard.crate_name,
                unexpected.join(", ")
            ));
        }

        if actual.len() > allowed.len() {
            violations.push(format!(
                "{} facade reexport count grew from allowed max {} to {}",
                guard.crate_name,
                allowed.len(),
                actual.len()
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "major facade crates must not grow broad `pub use` surfaces; migrate callers to owner crates or update docs/specs/crate-ownership.md with a deliberate integrator exception:\n{}",
        violations.join("\n")
    );
}

struct FacadeReexportGuard {
    crate_name: &'static str,
    relative_lib_rs: &'static str,
    allowed_reexports: &'static [&'static str],
}

const CATALOG_FACADE_REEXPORT_ALLOWLIST: &[&str] = &[
    "CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH",
    "CatalogChangeNotification",
    "CatalogChangeSubscription",
    "CatalogChangeSubscriptionCursor",
    "CatalogDefinitionBatchPlanning",
    "CatalogManifestRecord",
    "CatalogManifestResolution",
    "CatalogManifestResolutionFailure",
    "CatalogManifestResolutionRequest",
    "CatalogManifestResolutionStatus",
    "CatalogManifestRuntimeMetadata",
    "CatalogManifestSelector",
    "CatalogManifestStore",
    "CatalogManifestStoreBoundary",
    "CatalogMutation",
    "CatalogMutationBoundary",
    "CatalogMutationCommitEvidence",
    "CatalogMutationDelta",
    "CatalogMutationOperation",
    "CatalogMutationPlan",
    "CatalogMutationRecord",
    "CatalogMutationRecordKind",
    "CatalogPublicationReceipt",
    "CatalogRecoveryOutcome",
    "CatalogRuntimeEvidence",
    "CatalogRuntimeReopenEvidence",
    "CatalogRuntimeStore",
    "CatalogServerRuntime",
    "CatalogServerRuntimeDiagnostic",
    "CatalogServerRuntimeKind",
    "CatalogServerTrait",
    "CatalogSnapshot",
    "CatalogSnapshotManifestStore",
    "CatalogSubscriptionRegistry",
    "CatalogSystemApplyReport",
    "CatalogSystemDurableApplyReport",
    "CatalogSystemStore",
    "CatalogSystemWalAppend",
    "CatalogWalPayloadDecodeError",
    "CatalogWalPayloadDecodeErrorKind",
    "ColumnSchema",
    "DefinitionBatchPlan",
    "DurableCatalogRuntimeHandle",
    "ProcedureManifest",
    "recover_catalog_snapshot_from_durable_payloads",
    "replay_catalog_mutation_records",
    "require_durable_catalog_runtime",
];

const OBSERVE_FACADE_REEXPORT_ALLOWLIST: &[&str] = &[
    "AdminOperation",
    "AdminOperationTrace",
    "AdmissionAuditEvent",
    "AdmissionDecisionKind",
    "AffectedPrincipal",
    "AuditTrace",
    "AuthorizationDeniedTrace",
    "BackpressureReason",
    "BackpressureTrace",
    "BackupAuditEvent",
    "BackupAuditTrace",
    "BackupId",
    "CatalogMutationTrace",
    "CertificateIdentity",
    "CommitVisibleTrace",
    "CompletionEmittedTrace",
    "ContractRejectedTrace",
    "ContractValidationResult",
    "CorruptionBoundaryTrace",
    "DurableAuditAppendRecord",
    "DurableAuditCompactionReport",
    "DurableAuditDecisionGate",
    "DurableAuditEventFamily",
    "DurableAuditFailureKind",
    "DurableAuditPolicyEvidenceRequirement",
    "DurableAuditPrincipalBinding",
    "DurableAuditPruneBlockReason",
    "DurableAuditPruneEvidence",
    "DurableAuditRecordIdentity",
    "DurableAuditReplayBehavior",
    "DurableAuditReplayEvidence",
    "DurableAuditReplayLsnRange",
    "DurableAuditReplayQuery",
    "DurableAuditReplayRecord",
    "DurableAuditReplayResult",
    "DurableAuditReplayWindow",
    "DurableAuditRetentionBoundary",
    "DurableAuditRetentionManager",
    "DurableAuditRetentionPolicy",
    "DurableAuditSinkFailure",
    "DurableAuditSinkReport",
    "DurableAuditSinkResult",
    "DurableAuditTraceQueryResult",
    "DurableAuditTraceQueryRow",
    "DurableAuditTraceQuerySource",
    "DurableAuditVisibleDecisionProof",
    "DurableAuditWalEvidence",
    "DurableAuditWalSegmentArchiveProof",
    "DurableAuditWalSink",
    "EventEmitter",
    "EventEnvelope",
    "EventSink",
    "ExecutionTransitionTrace",
    "ExportDecisionTrace",
    "ExporterBackend",
    "ExporterConfig",
    "ExporterTrait",
    "FencingDecision",
    "FencingEvent",
    "FencingPolicy",
    "FileDurableAuditWalSink",
    "FrameRejectionTrace",
    "GpuPolicyDecisionTrace",
    "HadrAuditEvent",
    "HadrAuditTrace",
    "InMemoryEventSequence",
    "InMemoryEventSink",
    "InvocationTrace",
    "IoBudgetDecisionTrace",
    "IoPipelineStage",
    "IoPlacementDecisionTrace",
    "IoStorageTier",
    "ManifestEventKind",
    "ManifestTrace",
    "Metric",
    "MockExporter",
    "MvccTrace",
    "PendingDurableAuditRecord",
    "Permission",
    "PermissionFamily",
    "PlacementAuditEvent",
    "PlacementAuditTransition",
    "ProcedureId",
    "ProcedureLifecycleTrace",
    "PromotionCompletion",
    "PromotionEligibility",
    "ProtocolRejectionReason",
    "ProtocolRejectionTrace",
    "ProtocolSurfacePlane",
    "QuorumRole",
    "RecoveryStage",
    "RecoveryTrace",
    "ReplicaHealthState",
    "ResourceTrace",
    "RestoreCompletion",
    "RestoreCompletionStatus",
    "RestoreId",
    "RestoreTrace",
    "RetryPolicy",
    "RollbackDurableTrace",
    "SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID",
    "SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION",
    "SchemaLayoutDecisionTrace",
    "SecurityAdmissionAuditEventV0",
    "SecurityAuditDenialReason",
    "SecurityAuditOutcome",
    "SecurityAuditTrace",
    "SecurityPolicyVersionEvidence",
    "StreamRoleRejectionTrace",
    "SurfaceScope",
    "TraceEvent",
    "TraceQueryMetadata",
    "TraceQueryResult",
    "TraceQueryRow",
    "TransactionPhaseCode",
    "TransactionTransitionTrace",
    "TransitionReasonCode",
    "UnsupportedVersionTrace",
    "UserPrincipal",
    "UserPrincipalKind",
    "WalEventTrace",
    "WalOperation",
    "WalTrace",
    "classify_policy_evidence_requirement",
];

const TRANSACTION_FACADE_REEXPORT_ALLOWLIST: &[&str] = &[
    "CommitProtocol",
    "LockReleaseAllTrace",
    "TransactionEvent",
    "TransactionIdAllocator",
    "TransactionLockCoordinator",
    "TransactionManager",
    "TransactionRecord",
    "TransactionState",
    "TransactionStateMachine",
    "TransactionTrace",
    "TransactionTransitionCorrelation",
    "TwoPhaseLocksValidator",
    "TwoPhaseOperation",
    "TxWalAdapterTrait",
    "WalManager",
    "append_commit_and_flush",
    "transaction_phase_code",
];

#[test]
fn crate_ownership_cli_audit_tests_import_audit_and_observability_owners() {
    let workspace = workspace_root();
    let relative = "crates/andromeda-cli/tests/audit_cli_commands.rs";
    let file = workspace.join(relative);
    let source =
        fs::read_to_string(&file).unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
    let code = strip_rust_comments(&source);
    let forbidden_observe_imports = [
        "AdminOperation",
        "AdminOperationTrace",
        "CertificateIdentity",
        "DurableAuditPrincipalBinding",
        "DurableAuditReplayBehavior",
        "DurableAuditRetentionBoundary",
        "DurableAuditSinkReport",
        "DurableAuditWalSink",
        "EventCorrelation",
        "EventId",
        "FileDurableAuditWalSink",
        "Permission",
        "SecurityAuditOutcome",
        "SecurityAuditTrace",
        "SecurityPolicyVersionEvidence",
        "SurfaceScope",
        "TraceId",
        "UserPrincipal",
        "UserPrincipalKind",
    ];

    let mut violations = Vec::new();
    for item in forbidden_observe_imports {
        let direct_import = format!("andromeda_observe::{item}");
        if code.contains(&direct_import) {
            violations.push(format!(
                "{relative} imports `{direct_import}`; use the owner crate"
            ));
        }
    }
    violations.extend(forbidden_grouped_import_items(
        relative,
        &code,
        "andromeda_observe",
        &forbidden_observe_imports,
    ));

    assert!(
        violations.is_empty(),
        "CLI audit tests must import audit-owned types from andromeda-audit and observability primitives from andromeda-observability, while keeping observe-owned event/envelope surfaces on andromeda-observe:\n{}",
        violations.join("\n")
    );
}

#[test]
fn crate_ownership_catalog_tests_import_dtos_from_owner_crates() {
    let workspace = workspace_root();
    let catalog_tests_root = workspace.join("crates/andromeda-catalog/tests");
    let forbidden_catalog_facade_items = [
        "CatalogDefinition",
        "CatalogDependencyKind",
        "CatalogDurabilityMarker",
        "CatalogDurableMutationPayload",
        "CatalogLifecycleTarget",
        "CatalogManifestResolutionRequest",
        "CatalogManifestResolutionStatus",
        "CatalogMutationDurability",
        "CatalogMutationRecordKind",
        "CatalogPublicationSemantics",
        "CatalogRecoveryReport",
        "CatalogRuntimeReopenEvidence",
        "CatalogSkippedBatchReason",
        "CatalogSnapshotPublication",
        "DefinitionBatch",
        "DefinitionBatchId",
        "DefinitionBatchPlan",
        "DefinitionOperation",
        "ObjectKind",
        "QualifiedName",
        "StructuredObjectDefinition",
        "TableDefinition",
    ];
    let direct_replacements = [
        ("digest", "andromeda_digest"),
        (
            "recover_catalog_snapshot_from_durable_payloads",
            "andromeda_catalog_recovery::recover_catalog_target_from_durable_payloads",
        ),
    ];

    let mut violations = Vec::new();
    for file in rust_source_files(&catalog_tests_root) {
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
        let code = strip_rust_comments(&source);
        let relative = file
            .strip_prefix(&workspace)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");

        for item in forbidden_catalog_facade_items {
            let direct_import = format!("andromeda_catalog::{item}");
            if code.contains(&direct_import) {
                violations.push(format!(
                    "{relative} imports `{direct_import}`; use the owner crate"
                ));
            }
        }
        violations.extend(forbidden_grouped_import_items(
            &relative,
            &code,
            "andromeda_catalog",
            &forbidden_catalog_facade_items,
        ));

        for (bad, good) in direct_replacements {
            let direct_import = format!("andromeda_catalog::{bad}");
            if code.contains(&direct_import) {
                violations.push(format!(
                    "{relative} imports `{direct_import}`; use `{good}`"
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "catalog tests must import DTO, definition-batch, recovery, and digest-owned contracts from owner crates while keeping catalog runtime/store/integrator types on andromeda-catalog:\n{}",
        violations.join("\n")
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("andromeda-regression lives under crates/")
        .to_path_buf()
}

fn rust_source_files(root: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_source_files(root, &mut files);
    files.sort();
    files
}

fn collect_rust_source_files(current: &std::path::Path, files: &mut Vec<PathBuf>) {
    if !current.is_dir() {
        return;
    }

    for entry in
        fs::read_dir(current).unwrap_or_else(|err| panic!("read {}: {err}", current.display()))
    {
        let entry =
            entry.unwrap_or_else(|err| panic!("read entry in {}: {err}", current.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_rust_source_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn forbidden_token_violations(
    workspace: &std::path::Path,
    file: &std::path::Path,
    token: &str,
) -> Vec<String> {
    let source =
        fs::read_to_string(file).unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
    let code = strip_rust_comments(&source);
    let relative = file
        .strip_prefix(workspace)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/");

    code.lines()
        .enumerate()
        .filter(|(_, line)| line.contains(token))
        .map(|(index, _)| format!("{relative}:{} imports `{token}`", index + 1))
        .collect()
}

fn strip_rust_comments(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_line_comment = false;
    let mut block_comment_depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                result.push('\n');
            }
            continue;
        }

        if block_comment_depth > 0 {
            if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                block_comment_depth += 1;
            } else if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                block_comment_depth -= 1;
            } else if ch == '\n' {
                result.push('\n');
            }
            continue;
        }

        if in_string {
            result.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            result.push(ch);
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            in_line_comment = true;
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            block_comment_depth = 1;
            continue;
        }

        result.push(ch);
    }

    result
}

fn forbidden_grouped_import_items(
    relative: &str,
    code: &str,
    crate_name: &str,
    forbidden_items: &[&str],
) -> Vec<String> {
    let mut violations = Vec::new();
    let mut in_import = false;
    for (index, line) in code.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with(&format!("use {crate_name}::{{")) {
            in_import = true;
        }
        if in_import {
            for item in forbidden_items {
                if line
                    .split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
                    .any(|token| token == *item)
                {
                    violations.push(format!(
                        "{relative}:{} imports `{item}` through `{crate_name}`",
                        index + 1
                    ));
                }
            }
            if trimmed.ends_with("};") {
                in_import = false;
            }
        }
    }
    violations
}

fn facade_reexport_items(source: &str) -> BTreeSet<String> {
    let code = strip_rust_comments(source);
    let mut items = BTreeSet::new();
    let mut statement = String::new();
    let mut in_pub_use = false;

    for line in code.lines() {
        let trimmed = line.trim();
        if !in_pub_use && trimmed.starts_with("pub use ") {
            statement.clear();
            statement.push_str(trimmed);
            in_pub_use = !trimmed.ends_with(';');
            if in_pub_use {
                continue;
            }
        } else if in_pub_use {
            statement.push(' ');
            statement.push_str(trimmed);
            if trimmed.ends_with(';') {
                in_pub_use = false;
            } else {
                continue;
            }
        } else {
            continue;
        }

        collect_pub_use_items(&statement, &mut items);
    }

    items
}

fn collect_pub_use_items(statement: &str, items: &mut BTreeSet<String>) {
    let statement = statement.trim_end_matches(';').trim();
    if let Some((_, after_open_brace)) = statement.split_once('{') {
        let (inside_braces, _) = after_open_brace
            .rsplit_once('}')
            .unwrap_or_else(|| panic!("malformed grouped pub use statement: {statement}"));
        for raw_item in inside_braces.split(',') {
            let item = raw_item.trim();
            if !item.is_empty() {
                items.insert(pub_use_item_name(item).to_owned());
            }
        }
        return;
    }

    let item = statement
        .strip_prefix("pub use ")
        .unwrap_or_else(|| panic!("malformed pub use statement: {statement}"))
        .trim();
    items.insert(pub_use_item_name(item).to_owned());
}

fn pub_use_item_name(item: &str) -> &str {
    item.rsplit_once(" as ")
        .map(|(_, alias)| alias.trim())
        .unwrap_or(item)
        .rsplit("::")
        .next()
        .expect("pub use item name")
        .trim()
}
