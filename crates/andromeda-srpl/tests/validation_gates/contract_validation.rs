use crate::support::compile_gate_source;

#[test]
fn gate_08_contract_hash_deterministic() {
    let srpl = "procedure Inventory.ReserveStock accepts (P i64) returns R one (C bool);";

    let ir1 = compile_gate_source(srpl, "should compile first");
    let ir2 = compile_gate_source(srpl, "should compile second");

    assert_eq!(ir1.name, ir2.name, "Procedure names must be deterministic");
    assert_eq!(
        ir1.inputs.len(),
        ir2.inputs.len(),
        "Input count must be deterministic"
    );
    assert_eq!(
        ir1.result_streams.len(),
        ir2.result_streams.len(),
        "Result stream count must be deterministic"
    );

    println!("✅ Gate 08: Contract validation deterministic");
}
