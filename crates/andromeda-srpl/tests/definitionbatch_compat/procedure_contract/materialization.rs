use andromeda_catalog::{CatalogDefinition, ResultStreamCardinality};
use andromeda_core::{CatalogObjectId, CatalogVersion, ProcedureId};
use andromeda_srpl::definition_batch_bridge::SrplProcedureDefinition;

use crate::support::{bound_definition, signature_only_source, test_metadata};

#[test]
fn a1_parse_to_ast_round_trip() {
    let source = signature_only_source();
    let mut def = SrplProcedureDefinition::from_source(source.clone());

    assert!(def.parse().is_ok());
    assert!(def.parsed_ast.is_some());

    let ast = def.parsed_ast.unwrap();
    assert_eq!(ast.name.value.as_catalog_path(), "Inventory.ReserveStock");
}

#[test]
fn a2_ast_to_ir_compilation() {
    let source = signature_only_source();
    let mut def = SrplProcedureDefinition::from_source(source);

    assert!(def.parse().is_ok());
    assert!(def.bind_and_lower().is_ok());
    assert!(def.compiled_ir.is_some());

    let ir = def.compiled_ir.unwrap();
    assert_eq!(ir.name.as_catalog_path(), "Inventory.ReserveStock");
    assert_eq!(ir.inputs.len(), 1);
    assert_eq!(ir.result_streams.len(), 1);
}

#[test]
fn a3_ir_to_catalog_definition_materialization() {
    let catalog_def = bound_definition(signature_only_source())
        .to_catalog_procedure_def(
            CatalogObjectId::new(1),
            ProcedureId::new(1),
            CatalogVersion::new(1),
        )
        .expect("bound SRPL definition should materialize");

    let CatalogDefinition::Procedure(contract) = catalog_def else {
        panic!("bound SRPL source must materialize as a Procedure catalog definition");
    };
    assert_eq!(
        contract.object.name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(contract.inputs.len(), 1);
    assert_eq!(contract.result_streams.len(), 1);
    assert_eq!(
        contract.result_streams[0].cardinality,
        ResultStreamCardinality::One
    );
    assert!(contract.result_streams[0].row_count_exact_required);
}

#[test]
fn a6_staged_catalog_definition_with_metadata_matches_compiler_pipeline() {
    let source = signature_only_source();
    let metadata = test_metadata(60, 60, CatalogVersion::new(1));
    let def = bound_definition(source.clone());

    let staged = def
        .to_catalog_procedure_def_with_metadata(metadata.clone())
        .expect("staged materialization should succeed");
    let direct = andromeda_srpl::compile_narrow_procedure_definition(&source, metadata)
        .expect("direct pipeline materialization should succeed");

    assert_eq!(staged, direct);
}
