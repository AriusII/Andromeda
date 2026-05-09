use andromeda_catalog::CatalogSystemStore;
use andromeda_srpl_definition_batch::{
    SrplDefinitionBatchProcedureSource, dry_run_srpl_definition_batch_sources,
};
use andromeda_types::{CatalogVersion, ProcedureId};

use crate::support::{
    TEST_DB_ID, TEST_NS_ID, dry_run_request, signature_only_source, test_metadata,
};

#[test]
fn g5_contract_hash_is_stable_across_equivalent_source_formatting() {
    let source_a = signature_only_source();
    let source_b = "procedure Inventory.ReserveStock
        accepts   (ProductId i64)
        returns   Reservation one (Reserved bool);"
        .to_string();

    let report_a = dry_run_srpl_definition_batch_sources(dry_run_request(
        74,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_a,
            test_metadata(740, 740, CatalogVersion::new(1)),
        )],
    ))
    .expect("formatted source A should compile");
    let report_b = dry_run_srpl_definition_batch_sources(dry_run_request(
        75,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_b,
            test_metadata(740, 740, CatalogVersion::new(1)),
        )],
    ))
    .expect("formatted source B should compile");

    assert_eq!(
        report_a.manifests[0].binding.contract_hash,
        report_b.manifests[0].binding.contract_hash
    );
}

#[test]
fn g9_source_digest_distinguishes_exact_srpl_text_from_materialized_contract_hash() {
    let source_a = signature_only_source();
    let source_b = "procedure Inventory.ReserveStock
        accepts   (ProductId i64)
        returns   Reservation one (Reserved bool);"
        .to_string();

    let report_a = dry_run_srpl_definition_batch_sources(dry_run_request(
        80,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_a,
            test_metadata(800, 800, CatalogVersion::new(1)),
        )],
    ))
    .expect("source A should compile");
    let report_b = dry_run_srpl_definition_batch_sources(dry_run_request(
        80,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_b,
            test_metadata(800, 800, CatalogVersion::new(1)),
        )],
    ))
    .expect("source B should compile");

    let manifest_a = &report_a.manifests[0];
    let manifest_b = &report_b.manifests[0];

    assert_ne!(
        manifest_a.source_digest, manifest_b.source_digest,
        "SRPL source digest must bind exact source text"
    );
    assert_eq!(
        manifest_a.contract_hash, manifest_b.contract_hash,
        "ProcedureContract hash must be derived from canonical typed contract shape"
    );
    assert_eq!(manifest_a.contract_hash, manifest_a.binding.contract_hash);
    assert_eq!(manifest_b.contract_hash, manifest_b.binding.contract_hash);
    assert_eq!(
        report_a.definition_batch_source_hash, report_b.definition_batch_source_hash,
        "materialized DefinitionBatch source hash should ignore equivalent SRPL formatting"
    );
    assert_eq!(
        report_a.definition_batch_source_hash,
        report_a.definition_batch.source_hash()
    );
    assert_ne!(
        report_a.source_evidence.procedures[0].source_digest,
        report_b.source_evidence.procedures[0].source_digest,
        "apply/durable source evidence must bind exact SRPL text"
    );
    assert_eq!(
        report_a.source_evidence.procedures[0].contract_hash,
        report_b.source_evidence.procedures[0].contract_hash,
        "apply/durable source evidence must also bind the canonical Procedure contract"
    );
}

#[test]
fn g9b_source_digest_is_stable_for_same_source_and_changes_for_semantic_text_change() {
    let source_a = signature_only_source();
    let source_a_repeat = signature_only_source();
    let source_c = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved i64);"
        .to_string();

    let report_a = dry_run_srpl_definition_batch_sources(dry_run_request(
        83,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_a,
            test_metadata(830, 830, CatalogVersion::new(1)),
        )],
    ))
    .expect("source A should compile");
    let report_a_repeat = dry_run_srpl_definition_batch_sources(dry_run_request(
        83,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_a_repeat,
            test_metadata(830, 830, CatalogVersion::new(1)),
        )],
    ))
    .expect("repeated source A should compile");
    let report_c = dry_run_srpl_definition_batch_sources(dry_run_request(
        84,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_c,
            test_metadata(840, 840, CatalogVersion::new(1)),
        )],
    ))
    .expect("semantically changed source should compile");

    assert_eq!(
        report_a.source_evidence.procedures[0].source_digest,
        report_a_repeat.source_evidence.procedures[0].source_digest,
        "same SRPL source text must produce stable source evidence"
    );
    assert_ne!(
        report_a.source_evidence.procedures[0].source_digest,
        report_c.source_evidence.procedures[0].source_digest,
        "semantic text change must produce a different SRPL source digest"
    );
    assert_ne!(
        report_a.source_evidence.procedures[0].contract_hash,
        report_c.source_evidence.procedures[0].contract_hash,
        "semantic text change must also change canonical Procedure contract evidence"
    );
}

#[test]
fn g10_source_digest_evidence_survives_durable_catalog_apply_without_raw_source() {
    let report = dry_run_srpl_definition_batch_sources(dry_run_request(
        81,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            signature_only_source(),
            test_metadata(810, 810, CatalogVersion::new(1)),
        )],
    ))
    .expect("SRPL source should materialize before durable catalog apply");
    let expected_evidence = report.source_evidence.clone();
    let mut store = CatalogSystemStore::empty(TEST_DB_ID, TEST_NS_ID, CatalogVersion::new(0));
    let mut next_lsn = 200;
    let mut flushed_commit_lsn = None;

    let apply_report = report
        .apply_to_catalog_store_durably(
            &mut store,
            |_kind, payload| {
                let _payload_len = payload.len();
                let lsn = next_lsn;
                next_lsn += 1;
                Ok(lsn)
            },
            |commit_lsn| {
                flushed_commit_lsn = Some(commit_lsn);
                Ok(commit_lsn)
            },
        )
        .expect("durable catalog apply should carry SRPL source evidence");

    assert_eq!(apply_report.source_evidence, expected_evidence);
    assert_eq!(
        apply_report.catalog_report.source_hash,
        expected_evidence.definition_batch_source_hash
    );
    assert_eq!(
        apply_report.catalog_report.dependency_graph_hash,
        expected_evidence.definition_batch_dependency_graph_hash
    );
    assert_eq!(
        apply_report.source_evidence.procedures[0]
            .procedure_name
            .as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(
        apply_report.source_evidence.procedures[0].procedure_id,
        ProcedureId::new(810)
    );
    assert!(
        !apply_report.source_evidence.procedures[0]
            .source_digest
            .is_zero()
    );
    assert_eq!(
        apply_report.catalog_report.receipt.durable_lsn,
        flushed_commit_lsn
    );
    assert_eq!(flushed_commit_lsn, Some(202));
    assert_eq!(store.snapshot().version, CatalogVersion::new(1));
}

#[test]
fn g11_source_digest_evidence_survives_durable_catalog_apply() {
    let report = dry_run_srpl_definition_batch_sources(dry_run_request(
        82,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            signature_only_source(),
            test_metadata(820, 820, CatalogVersion::new(1)),
        )],
    ))
    .expect("SRPL source should materialize before durable catalog apply");
    let expected_evidence = report.source_evidence.clone();
    let mut store = CatalogSystemStore::empty(TEST_DB_ID, TEST_NS_ID, CatalogVersion::new(0));
    let mut next_lsn = 100;
    let mut flushed_commit_lsn = None;

    let durable_report = report
        .apply_to_catalog_store_durably(
            &mut store,
            |_kind, payload| {
                let _payload_len = payload.len();
                let lsn = next_lsn;
                next_lsn += 1;
                Ok(lsn)
            },
            |commit_lsn| {
                flushed_commit_lsn = Some(commit_lsn);
                Ok(commit_lsn)
            },
        )
        .expect("durable catalog apply should carry SRPL source evidence");

    assert_eq!(durable_report.source_evidence, expected_evidence);
    assert_eq!(
        durable_report.catalog_report.source_hash,
        expected_evidence.definition_batch_source_hash
    );
    assert_eq!(
        durable_report.catalog_report.dependency_graph_hash,
        expected_evidence.definition_batch_dependency_graph_hash
    );
    assert_eq!(
        durable_report.catalog_report.receipt.durable_lsn,
        flushed_commit_lsn
    );
    assert_eq!(flushed_commit_lsn, Some(102));
    assert!(
        !durable_report.source_evidence.procedures[0]
            .contract_hash
            .is_zero()
    );
    assert_eq!(store.snapshot().version, CatalogVersion::new(1));
}
