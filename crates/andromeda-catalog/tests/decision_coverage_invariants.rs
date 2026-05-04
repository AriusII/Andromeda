#[test]
fn decision_coverage_dec_022_covers_alter_procedure_lifecycle_before_operation_surface_expands() {
    let decision_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("decisions")
        .join("DEC-022-alter-procedure-lifecycle.md");
    let decision =
        std::fs::read_to_string(decision_path).expect("DEC-022 must exist for Alter Procedure");

    for required in [
        "Alter Procedure",
        "Compatibility Policy",
        "ContractHash",
        "Dependency Traversal",
        "Restrict/Cascade Interaction",
        "Active Invocation Boundary",
        "Plan-Cache Invalidation Implications",
        "Future Audit/WAL Requirements",
        "Drop semantics and cascade semantics for removing objects remain out of scope",
    ] {
        assert!(
            decision.contains(required),
            "DEC-022 must cover required Alter Procedure topic: {required}"
        );
    }
}
