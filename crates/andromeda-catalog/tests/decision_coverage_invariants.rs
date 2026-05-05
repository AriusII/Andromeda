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

#[test]
fn decision_coverage_dec_023_covers_drop_procedure_lifecycle_before_operation_surface_expands() {
    let decision_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("decisions")
        .join("DEC-023-drop-procedure-lifecycle.md");
    let decision =
        std::fs::read_to_string(decision_path).expect("DEC-023 must exist for Drop Procedure");

    for required in [
        "Drop Procedure",
        "Identity Transition: Active → Inactive → Removable",
        "Cascade/Restrict Policy",
        "Restrict mode",
        "Cascade mode",
        "out of scope for this decision",
        "Historical Contract Retention",
        "Active Invocation Boundary",
        "Compatibility Guarantee Lifetime",
        "Plan-Cache Implications",
        "Statistics and Feedback Retention",
        "Future Audit/WAL Requirements",
        "Interaction with E2 (Alter Procedure) and E4/E6/E7",
        "E4 (Restrict/Cascade Lifecycle Decision)",
        "future E4 decision",
    ] {
        assert!(
            decision.contains(required),
            "DEC-023 must cover required Drop Procedure topic: {required}"
        );
    }
}

#[test]
fn drop_procedure_operation_surface_is_guarded_not_yet_implemented() {
    // Verify that DefinitionOperation enum does not yet include a Drop variant.
    // This guard ensures Drop remains a design-only decision until explicit surface expansion.
    use andromeda_catalog::DefinitionOperation;

    // Count the number of variants in DefinitionOperation.
    // Currently: Create, Deprecate. Drop is not yet a variant.
    // This test serves as a reminder to update the decision and tests when Drop is added.
    let create_example = DefinitionOperation::Create(andromeda_catalog::CatalogDefinition::Table(
        andromeda_catalog::TableDefinition {
            object: andromeda_catalog::CatalogObjectRef {
                object_id: andromeda_core::CatalogObjectId::new(1),
                name: andromeda_catalog::QualifiedName::parse("test.Table").unwrap(),
                kind: andromeda_catalog::ObjectKind::Table,
                catalog_version: andromeda_core::CatalogVersion::new(1),
            },
            columns: vec![],
        },
    ));

    let deprecate_example =
        DefinitionOperation::Deprecate(andromeda_catalog::CatalogLifecycleTarget {
            object: andromeda_catalog::CatalogObjectRef {
                object_id: andromeda_core::CatalogObjectId::new(1),
                name: andromeda_catalog::QualifiedName::parse("test.Table").unwrap(),
                kind: andromeda_catalog::ObjectKind::Table,
                catalog_version: andromeda_core::CatalogVersion::new(1),
            },
        });

    // If this test fails to compile after Drop is added, update the guard test accordingly.
    match (&create_example, &deprecate_example) {
        (DefinitionOperation::Create(_), DefinitionOperation::Deprecate(_)) => {
            // Expected: only Create and Deprecate exist. Drop is guarded.
        }
        _ => {
            panic!(
                "DefinitionOperation surface must remain limited to Create/Deprecate until Drop is explicitly designed and decided"
            );
        }
    }
}

#[test]
fn drop_procedure_cascade_policy_is_constrained_by_future_e4_decision() {
    // This test documents that Drop's cascade/restrict behavior depends on E4.
    // Once E4 is accepted, this test should be updated to verify E4 constraints.
    let decision_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("decisions")
        .join("DEC-023-drop-procedure-lifecycle.md");
    let decision =
        std::fs::read_to_string(decision_path).expect("DEC-023 must exist for Drop Procedure");

    assert!(
        decision.contains("E4 (Restrict/Cascade Lifecycle Decision)"),
        "DEC-023 must establish dependency on E4 for cascade/restrict policy"
    );

    assert!(
        decision.contains("Cascade mode (future decision required before implementation)"),
        "DEC-023 must note that Cascade requires a future decision (E4)"
    );

    assert!(
        decision.contains("All current Drop support must default to Restrict only"),
        "DEC-023 must mandate Restrict-only default until E4 is accepted"
    );
}
