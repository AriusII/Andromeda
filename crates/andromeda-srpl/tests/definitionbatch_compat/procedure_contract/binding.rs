use andromeda_core::{ScalarType, TypeDescriptor};

use crate::support::{bound_definition, signature_only_source};

#[test]
fn c1_alter_srpl_procedure_compiles_new_source() {
    let old_ir = bound_definition(signature_only_source())
        .compiled_ir
        .expect("old source should lower to IR");
    let new_ir = bound_definition(
        "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool);"
            .to_string(),
    )
    .compiled_ir
    .expect("new source should lower to IR");

    assert_eq!(old_ir.inputs.len(), 1);
    assert_eq!(new_ir.inputs.len(), 2);
}

#[test]
fn f3_single_result_stream_survives_parse_and_lower() {
    let ir = bound_definition(signature_only_source())
        .compiled_ir
        .expect("source should lower to IR");

    assert_eq!(ir.result_streams.len(), 1);
    assert_eq!(ir.result_streams[0].name, "Reservation");
    assert_eq!(ir.result_streams[0].columns.len(), 1);
    assert_eq!(ir.result_streams[0].columns[0].name, "Reserved");
}

#[test]
fn f4_declared_scalar_types_survive_binding() {
    let ir = bound_definition(signature_only_source())
        .compiled_ir
        .expect("source should lower to IR");

    assert_eq!(
        ir.inputs[0].data_type,
        TypeDescriptor::required(ScalarType::I64)
    );
    assert_eq!(
        ir.result_streams[0].columns[0].data_type,
        TypeDescriptor::required(ScalarType::Bool)
    );
}

#[test]
fn f5_body_read_operation_lowers_without_catalog_lookup() {
    let ir = bound_definition(
        "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; emit Reservation (Reserved); }"
            .to_string(),
    )
    .compiled_ir
    .expect("source with body should lower to IR");

    assert_eq!(ir.body.operations.len(), 2);
}
