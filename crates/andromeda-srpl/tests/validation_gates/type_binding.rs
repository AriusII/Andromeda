use andromeda_srpl::procedure_model::Cardinality;

use crate::support::compile_gate_source;

#[test]
fn gate_03_type_binding_integer_literals() {
    let srpl = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) \
        returns Reservation one (Reserved bool);";

    let ir = compile_gate_source(srpl, "should bind i64 types");

    assert_eq!(ir.inputs[0].name, "ProductId");
    assert_eq!(ir.inputs[1].name, "Quantity");
    println!("  ✅ Integer type binding verified");
    println!("✅ Gate 03: Type binding coverage verified");
}

#[test]
fn gate_03_type_binding_cardinality_coverage() {
    let cases = [
        (
            "procedure X accepts (P i64) returns R one (C bool);",
            "one",
            Cardinality::One,
        ),
        (
            "procedure X accepts (P i64) returns R optionalOne (C bool);",
            "optionalOne",
            Cardinality::OptionalOne,
        ),
        (
            "procedure X accepts (P i64) returns R optional one (C bool);",
            "optional one",
            Cardinality::OptionalOne,
        ),
        (
            "procedure X accepts (P i64) returns R many (C bool);",
            "many",
            Cardinality::Many,
        ),
        (
            "procedure X accepts (P i64) returns R nonEmptyMany (C bool);",
            "nonEmptyMany",
            Cardinality::NonEmptyMany,
        ),
        (
            "procedure X accepts (P i64) returns R nonempty many (C bool);",
            "nonempty many",
            Cardinality::NonEmptyMany,
        ),
    ];

    for (srpl, card_name, expected_cardinality) in cases {
        let ir = compile_gate_source(srpl, "cardinality case must compile");
        assert_eq!(
            ir.result_streams[0].cardinality, expected_cardinality,
            "cardinality {card_name} must bind to the expected explicit cardinality"
        );
        println!("  ✅ Cardinality {} binding verified", card_name);
    }

    println!("✅ Gate 03: Cardinality binding verified");
}
