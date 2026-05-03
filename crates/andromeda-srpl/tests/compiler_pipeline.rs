use andromeda_catalog::QualifiedName;
use andromeda_core::{ColumnDescriptor, ScalarType, TypeDescriptor};
use andromeda_srpl::{
    SourceSpan,
    compiler::{compile_narrow_procedure_signature, parse_procedure_signature},
    diagnostics::DiagnosticPhase,
    model::{Cardinality, ProcedureSignature, ResultContract, SrplProcedureIr},
    source::SrplSource,
};

#[test]
fn successful_narrow_procedure_compiles_to_ir() {
    let ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);",
    )
    .unwrap();

    assert_eq!(ir.name.as_catalog_path(), "Inventory.ReserveStock");
    assert_eq!(ir.inputs.len(), 1);
    assert_eq!(ir.inputs[0].name, "ProductId");
    assert_eq!(ir.result_streams.len(), 1);
    assert_eq!(ir.result_streams[0].name, "Reservation");
    assert_eq!(ir.result_streams[0].cardinality, Cardinality::One);
    assert_eq!(ir.result_streams[0].columns[0].name, "Reserved");
}

#[test]
fn parser_diagnostics_include_phase_and_span() {
    let diagnostic =
        parse_procedure_signature("procedure X accepts () returns R many ();").unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
    assert!(diagnostic.location.is_some());
    assert!(diagnostic.location.unwrap().is_valid());
    assert!(diagnostic.message.contains("at least one column"));
}

#[test]
fn forbidden_constructs_reject_before_lowering() {
    let diagnostic = compile_narrow_procedure_signature(
        "procedure X accepts () returns R many (C bool); execute sql",
    )
    .unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
    assert!(diagnostic.location.is_some());
    assert!(diagnostic.message.contains("dynamic text SQL"));
}

#[test]
fn duplicate_parameter_names_reject_with_binding_span() {
    let diagnostic = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64, ProductId i64) returns Reservation one (Reserved bool);",
    )
    .unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
    assert!(diagnostic.location.is_some());
    assert!(diagnostic.message.contains("input names must be unique"));
}

#[test]
fn duplicate_result_names_reject_in_public_contract_model() {
    let signature = ProcedureSignature {
        name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
        accepts: Vec::new(),
        returns: vec![
            ResultContract {
                name: "Reservation".to_string(),
                cardinality: Cardinality::One,
                columns: vec![ColumnDescriptor {
                    name: "Reserved".to_string(),
                    data_type: TypeDescriptor::required(ScalarType::Bool),
                    ordinal: 0,
                }],
            },
            ResultContract {
                name: "Reservation".to_string(),
                cardinality: Cardinality::OptionalOne,
                columns: vec![ColumnDescriptor {
                    name: "AlreadyReserved".to_string(),
                    data_type: TypeDescriptor::required(ScalarType::Bool),
                    ordinal: 0,
                }],
            },
        ],
    };

    let error = signature.validate().unwrap_err();

    assert!(
        error
            .message()
            .contains("result stream names must be unique")
    );
}

#[test]
fn public_api_is_consumable_from_outside_the_crate() {
    let source = SrplSource::new("procedure X accepts () returns R many (C bool);");
    assert!(source.forbidden_construct_diagnostics().is_empty());

    let span = SourceSpan::new(0, source.text.len());
    assert!(span.is_valid());

    let ir: SrplProcedureIr = andromeda_srpl::compile_narrow_procedure_signature(source.text)
        .expect("root re-export remains public");
    assert_eq!(
        ir.result_streams[0].cardinality,
        andromeda_srpl::Cardinality::Many
    );
}
