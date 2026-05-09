use std::collections::BTreeMap;

use andromeda_srpl_definition_batch::{
    MAX_SRPL_DEFINITION_BATCH_PROCEDURES, SrplDefinitionBatchProcedureSource,
    dry_run_srpl_definition_batch_sources,
};
use andromeda_types::{CatalogObjectId, CatalogVersion, ProcedureId};

use crate::support::{
    dry_run_request, lookup_signature_source, signature_only_source, test_metadata,
};

#[test]
fn g3_source_dry_run_materializes_definition_batch_and_manifest() {
    let result = dry_run_srpl_definition_batch_sources(dry_run_request(
        72,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            signature_only_source(),
            test_metadata(720, 720, CatalogVersion::new(1)),
        )],
    ))
    .expect("valid SRPL source should materialize and dry-run");

    assert_eq!(result.definition_batch.operations.len(), 1);
    assert_eq!(result.plan.operation_count, 1);
    assert_eq!(result.manifests.len(), 1);
    assert_eq!(
        result.definition_batch_source_hash,
        result.definition_batch.source_hash()
    );
    assert_eq!(
        result.definition_batch_dependency_graph_hash,
        result.definition_batch.dependency_graph_hash().unwrap()
    );
    assert!(!result.definition_batch_source_hash.is_zero());
    assert!(!result.definition_batch_dependency_graph_hash.is_zero());

    let manifest = &result.manifests[0];
    assert_eq!(manifest.source_index, 0);
    assert_eq!(
        manifest.procedure_name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(manifest.object_id, CatalogObjectId::new(720));
    assert_eq!(manifest.binding.procedure_id, ProcedureId::new(720));
    assert_eq!(manifest.binding.catalog_version, CatalogVersion::new(1));
    assert_eq!(manifest.contract_hash, manifest.binding.contract_hash);
    assert!(!manifest.source_digest.is_zero());
    assert_eq!(manifest.input_count, 1);
    assert_eq!(manifest.result_stream_count, 1);
    assert!(manifest.binding.validate().is_ok());

    assert_eq!(
        result.source_evidence.definition_batch_source_hash,
        result.definition_batch_source_hash
    );
    assert_eq!(
        result
            .source_evidence
            .definition_batch_dependency_graph_hash,
        result.definition_batch_dependency_graph_hash
    );
    assert_eq!(result.source_evidence.procedures.len(), 1);
    assert_eq!(
        result.source_evidence.procedures[0].source_digest,
        manifest.source_digest
    );
    assert_eq!(
        result.source_evidence.procedures[0].contract_hash,
        manifest.contract_hash
    );
}

#[test]
fn g4_catalog_conflict_rejects_after_all_sources_materialize() {
    let result = dry_run_srpl_definition_batch_sources(dry_run_request(
        73,
        CatalogVersion::new(0),
        vec![
            SrplDefinitionBatchProcedureSource::new(
                signature_only_source(),
                test_metadata(730, 730, CatalogVersion::new(1)),
            ),
            SrplDefinitionBatchProcedureSource::new(
                signature_only_source(),
                test_metadata(731, 731, CatalogVersion::new(1)),
            ),
        ],
    ));

    let error = result.expect_err("duplicate materialized Procedure names must reject the batch");
    assert_eq!(error.diagnostics.len(), 1);
    assert_eq!(error.diagnostics[0].source_index, None);
    assert!(
        error.diagnostics[0]
            .diagnostic
            .message
            .contains("materialized DefinitionBatch was rejected by catalog dry-run")
    );
}

#[test]
fn g4b_source_dry_run_rejects_unbounded_procedure_source_count() {
    let procedures = (0..=MAX_SRPL_DEFINITION_BATCH_PROCEDURES)
        .map(|index| {
            SrplDefinitionBatchProcedureSource::new(
                signature_only_source(),
                test_metadata(
                    7_400 + index as u64,
                    7_500 + index as u64,
                    CatalogVersion::new(1),
                ),
            )
        })
        .collect();

    let error = dry_run_srpl_definition_batch_sources(dry_run_request(
        7_399,
        CatalogVersion::new(0),
        procedures,
    ))
    .expect_err("SRPL DefinitionBatch dry-run must reject unbounded source counts");

    assert_eq!(error.diagnostics.len(), 1);
    assert_eq!(error.diagnostics[0].source_index, None);
    assert!(
        error.diagnostics[0]
            .diagnostic
            .message
            .contains("accepts at most")
    );
}

#[test]
fn g8_manifest_bindings_are_stable_when_source_order_changes() {
    let reserve = SrplDefinitionBatchProcedureSource::new(
        signature_only_source(),
        test_metadata(780, 780, CatalogVersion::new(1)),
    );
    let lookup = SrplDefinitionBatchProcedureSource::new(
        lookup_signature_source(),
        test_metadata(781, 781, CatalogVersion::new(1)),
    );

    let forward = dry_run_srpl_definition_batch_sources(dry_run_request(
        78,
        CatalogVersion::new(0),
        vec![reserve.clone(), lookup.clone()],
    ))
    .expect("forward source order should dry-run");
    let reversed = dry_run_srpl_definition_batch_sources(dry_run_request(
        79,
        CatalogVersion::new(0),
        vec![lookup, reserve],
    ))
    .expect("reversed source order should dry-run");

    let forward_by_name = forward
        .manifests
        .iter()
        .map(|manifest| {
            (
                manifest.procedure_name.as_catalog_path(),
                (
                    manifest.object_id,
                    manifest.binding,
                    manifest.input_count,
                    manifest.result_stream_count,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let reversed_by_name = reversed
        .manifests
        .iter()
        .map(|manifest| {
            (
                manifest.procedure_name.as_catalog_path(),
                (
                    manifest.object_id,
                    manifest.binding,
                    manifest.input_count,
                    manifest.result_stream_count,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();

    assert_eq!(forward_by_name, reversed_by_name);
    assert_ne!(
        forward.definition_batch_source_hash, reversed.definition_batch_source_hash,
        "ordered DefinitionBatch source identity must change when source order changes"
    );
    assert_eq!(
        forward.definition_batch_dependency_graph_hash,
        reversed.definition_batch_dependency_graph_hash,
        "canonical dependency graph identity must ignore equivalent source ordering"
    );
    assert_eq!(forward.manifests[0].source_index, 0);
    assert_eq!(reversed.manifests[0].source_index, 0);
    assert_ne!(
        forward.manifests[0].procedure_name,
        reversed.manifests[0].procedure_name
    );
}
