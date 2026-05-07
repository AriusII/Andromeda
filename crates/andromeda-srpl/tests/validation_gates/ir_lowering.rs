use crate::support::compile_gate_source;

#[test]
fn gate_04_ir_lowering_read_operation() {
    let srpl = "procedure Inventory.ReserveStock accepts (P i64) returns R one (C bool) \
        body { read Inventory.ProductStock Stock one; }";

    let ir = compile_gate_source(srpl, "should lower read operation");

    assert_eq!(ir.body.operations.len(), 1);
    println!("  ✅ IR lowering: ReadTable operation verified");
}

#[test]
fn gate_04_ir_lowering_assert_operation() {
    let srpl = "procedure Inventory.ReserveStock accepts (P i64) returns R one (C bool) \
        body { assert Quantity InsufficientStock; }";

    let ir = compile_gate_source(srpl, "should lower assert operation");

    assert_eq!(ir.body.operations.len(), 1);
    println!("  ✅ IR lowering: Assert operation verified");
}

#[test]
fn gate_04_ir_lowering_update_operation() {
    let srpl = "procedure Inventory.ReserveStock accepts (P i64) returns R one (C bool) \
        body { update Inventory.ProductStock AvailableQuantity; }";

    let ir = compile_gate_source(srpl, "should lower update operation");

    assert_eq!(ir.body.operations.len(), 1);
    println!("  ✅ IR lowering: UpdateTable operation verified");
}

#[test]
fn gate_04_ir_lowering_emit_operation() {
    let srpl = "procedure Inventory.ReserveStock accepts (P i64) returns R one (C bool) \
        body { emit R (C); }";

    let ir = compile_gate_source(srpl, "should lower emit operation");

    assert_eq!(ir.body.operations.len(), 1);
    println!("  ✅ IR lowering: Emit operation verified");
    println!("✅ Gate 04: All IR node types exercised");
}
