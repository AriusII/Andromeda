use andromeda_catalog::{CatalogDefinition, DefinitionOperation};
use andromeda_contract::ResultStreamCardinality;
use andromeda_srpl::definition_batch_bridge::{
    SrplDefinitionBatchProcedureSource, dry_run_srpl_definition_batch_sources,
};
use andromeda_srpl_diagnostics::DiagnosticPhase;
use andromeda_types::{AbsencePolicy, CatalogObjectId, CatalogVersion, ProcedureId};

use crate::support::{dry_run_request, test_metadata};

#[derive(Clone, Copy)]
struct CardinalityCase {
    source: &'static str,
    expected_cardinality: ResultStreamCardinality,
    expected_exact_required: bool,
}

const CARDINALITY_CASES: &[CardinalityCase] = &[
    CardinalityCase {
        source: "procedure Inventory.ExactlyOne accepts (P i64) returns R one (C bool);",
        expected_cardinality: ResultStreamCardinality::One,
        expected_exact_required: true,
    },
    CardinalityCase {
        source: "procedure Inventory.OptionalResult accepts (P i64) returns R optional one (C bool);",
        expected_cardinality: ResultStreamCardinality::OptionalOne,
        expected_exact_required: false,
    },
    CardinalityCase {
        source: "procedure Inventory.NonEmptyResult accepts (P i64) returns R nonempty many (C bool);",
        expected_cardinality: ResultStreamCardinality::NonEmptyMany,
        expected_exact_required: true,
    },
    CardinalityCase {
        source: "procedure Inventory.ManyResults accepts (P i64) returns R many (C bool);",
        expected_cardinality: ResultStreamCardinality::Many,
        expected_exact_required: false,
    },
];

#[test]
fn a4b_definitionbatch_dry_run_rejects_ambiguous_cardinality_before_catalog_plan() {
    let source =
        "procedure Inventory.BadOptional accepts (ProductId i64) returns R optional many (C bool);";
    let request = dry_run_request(
        44,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source,
            test_metadata(44, 44, CatalogVersion::new(1)),
        )],
    );

    let error = dry_run_srpl_definition_batch_sources(request)
        .expect_err("ambiguous SRPL cardinality must fail before DefinitionBatch publication");

    assert_eq!(error.diagnostics.len(), 1);
    let diagnostic = &error.diagnostics[0].diagnostic;
    assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
    assert!(
        diagnostic
            .message
            .contains("optional cardinality must be written as `optional one`")
    );
}

#[test]
fn a5_staged_catalog_definition_preserves_full_result_cardinality() {
    for (index, case) in CARDINALITY_CASES.iter().enumerate() {
        let catalog_def = crate::support::bound_definition(case.source.to_string())
            .to_catalog_procedure_def(
                CatalogObjectId::new(10 + index as u64),
                ProcedureId::new(10 + index as u64),
                CatalogVersion::new(1),
            )
            .expect("case should materialize");

        let CatalogDefinition::Procedure(contract) = catalog_def else {
            panic!("staged SRPL source must materialize as a Procedure catalog definition");
        };
        assert_eq!(
            contract.result_streams[0].cardinality, case.expected_cardinality,
            "unexpected full cardinality for {}",
            case.source
        );
        assert_eq!(
            contract.result_streams[0].row_count_exact_required, case.expected_exact_required,
            "unexpected exact row-count flag for {}",
            case.source
        );
    }
}

#[test]
fn a5b_definitionbatch_dry_run_preserves_full_result_cardinality() {
    let report = dry_run_srpl_definition_batch_sources(dry_run_request(
        45,
        CatalogVersion::new(0),
        CARDINALITY_CASES
            .iter()
            .enumerate()
            .map(|(index, case)| {
                SrplDefinitionBatchProcedureSource::new(
                    case.source,
                    test_metadata(
                        450 + index as u64,
                        450 + index as u64,
                        CatalogVersion::new(1),
                    ),
                )
            })
            .collect(),
    ))
    .expect("cardinality probe batch should dry-run");

    assert_eq!(report.manifests.len(), CARDINALITY_CASES.len());
    assert_eq!(
        report.definition_batch.operations.len(),
        CARDINALITY_CASES.len()
    );
    for (index, case) in CARDINALITY_CASES.iter().enumerate() {
        assert_eq!(
            report.manifests[index].result_stream_cardinalities,
            vec![case.expected_cardinality],
            "manifest cardinality should survive for case {index}"
        );
        let DefinitionOperation::Create(CatalogDefinition::Procedure(contract)) =
            &report.definition_batch.operations[index]
        else {
            panic!("cardinality probe must materialize as Procedure definitions");
        };
        assert_eq!(
            contract.result_streams[0].cardinality, case.expected_cardinality,
            "DefinitionBatch contract cardinality should survive for case {index}"
        );
    }
}

#[test]
fn a5c_optional_one_does_not_introduce_implicit_null_semantics() {
    let report = dry_run_srpl_definition_batch_sources(dry_run_request(
        46,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            "procedure Inventory.OptionalResult accepts (P i64) returns R optional one (C bool);",
            test_metadata(460, 460, CatalogVersion::new(1)),
        )],
    ))
    .expect("optional-one result should dry-run without nullable columns");

    let DefinitionOperation::Create(CatalogDefinition::Procedure(contract)) =
        &report.definition_batch.operations[0]
    else {
        panic!("optional-one source must materialize as a Procedure contract");
    };
    let stream = &contract.result_streams[0];
    assert_eq!(stream.cardinality, ResultStreamCardinality::OptionalOne);
    assert_eq!(stream.columns[0].data_type.absence, AbsencePolicy::Required);

    let rejected = dry_run_srpl_definition_batch_sources(dry_run_request(
        47,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            "procedure Inventory.BadNull accepts (P i64) returns R optional one (C null);",
            test_metadata(470, 470, CatalogVersion::new(1)),
        )],
    ))
    .expect_err("SRPL null type must not become an implicit optional column");

    assert_eq!(rejected.diagnostics.len(), 1);
    let diagnostic = &rejected.diagnostics[0].diagnostic;
    assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
    assert!(
        diagnostic.message.contains("forbids nullable values"),
        "unexpected diagnostic: {}",
        diagnostic.message
    );
}
