use andromeda_procedure_contract::QualifiedName;
use andromeda_srpl_ir::{
    Cardinality, ResultContract, SrplEmitValueIr, SrplValueIr, validate_srpl_identifier,
};
use andromeda_types::{ColumnDescriptor, ScalarType, TypeDescriptor};

fn bool_column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::Bool),
        ordinal,
    }
}

#[test]
fn owner_ir_validates_identifiers_and_result_contracts() {
    validate_srpl_identifier("ReserveStock", "test SRPL identifier").unwrap();
    assert!(validate_srpl_identifier("not-valid", "test SRPL identifier").is_err());

    let result = ResultContract {
        name: "Reservation".to_string(),
        cardinality: Cardinality::One,
        columns: vec![bool_column("Reserved", 0)],
    };

    result.validate().unwrap();
}

#[test]
fn owner_ir_public_values_preserve_declared_columns() {
    let value = SrplEmitValueIr {
        column: "Reserved".to_string(),
        value: SrplValueIr::bool(true),
    };

    assert_eq!(value.column, "Reserved");
    assert!(value.value.is_constant());
}

#[test]
fn owner_ir_qualified_signature_type_is_constructible() {
    let name = QualifiedName::parse("Inventory.ReserveStock").unwrap();

    assert_eq!(name.as_catalog_path(), "Inventory.ReserveStock");
}
