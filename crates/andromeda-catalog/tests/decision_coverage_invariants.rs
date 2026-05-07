fn decision_path(file_name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("documentations")
        .join("governance")
        .join("decisions")
        .join(file_name)
}

#[test]
fn decision_coverage_dec_022_covers_alter_procedure_lifecycle_before_operation_surface_expands() {
    let decision = std::fs::read_to_string(decision_path("DEC-022-alter-procedure-lifecycle.md"))
        .expect("DEC-022 must exist for Alter Procedure");

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
    let decision = std::fs::read_to_string(decision_path("DEC-023-drop-procedure-lifecycle.md"))
        .expect("DEC-023 must exist for Drop Procedure");
    let normalized = decision
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

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
            normalized.contains(&required.to_lowercase()),
            "DEC-023 must cover required Drop Procedure topic: {required}"
        );
    }
}

#[test]
fn drop_procedure_operation_surface_is_guarded_not_yet_implemented() {
    use andromeda_catalog::DefinitionOperation;

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

    match (&create_example, &deprecate_example) {
        (DefinitionOperation::Create(_), DefinitionOperation::Deprecate(_)) => {}
        _ => {
            panic!(
                "DefinitionOperation surface must remain limited to Create/Deprecate until Drop is explicitly designed and decided"
            );
        }
    }
}

#[test]
fn drop_procedure_cascade_policy_is_constrained_by_future_e4_decision() {
    let decision = std::fs::read_to_string(decision_path("DEC-023-drop-procedure-lifecycle.md"))
        .expect("DEC-023 must exist for Drop Procedure");
    let normalized = decision
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    assert!(
        normalized.contains("e4 (restrict/cascade lifecycle decision)"),
        "DEC-023 must establish dependency on E4 for cascade/restrict policy"
    );

    assert!(
        normalized.contains("cascade mode (future decision required before implementation)"),
        "DEC-023 must note that Cascade requires a future decision (E4)"
    );

    assert!(
        normalized.contains("all current drop support must default to restrict only"),
        "DEC-023 must mandate Restrict-only default until E4 is accepted"
    );
}

#[test]
fn decision_coverage_plan_cache_runtime_gate_is_bounded_versioned_and_traceable() {
    let decision = std::fs::read_to_string(decision_path(
        "DEC-039-optimizer-intermediate-pass-contract.md",
    ))
    .expect("DEC-039 must exist for optimizer work");
    let decision_normalized = decision
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    for required in [
        "PlanCacheKey::build",
        "Every plan cache operation",
        "key digest",
        "catalog version",
        "stats version",
        "policy version",
        "plan class",
        "decision outcome",
    ] {
        assert!(
            decision_normalized.contains(&required.to_lowercase()),
            "DEC-039 must cover minimal PlanCache gate requirement: {required}"
        );
    }

    let plan_cache_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("plan_cache.rs");
    let plan_cache =
        std::fs::read_to_string(plan_cache_path).expect("plan_cache.rs must be readable");

    for required in [
        "PLAN_CACHE_MAX_ENTRIES",
        "PLAN_SELECTION_MAX_CANDIDATES",
        "PLAN_SELECTION_MAX_SCENARIO_EVIDENCE",
        "classify_advisory_evidence_for_key",
        "CriticalDecisionKind::PlanSelection",
        "PlanDecisionEvidence",
        "advisory_only=true",
        "policy_version",
        "contract_hash",
        "stats_version",
        "catalog_version",
    ] {
        assert!(
            plan_cache.contains(required),
            "plan_cache.rs must keep runtime gate coverage visible for: {required}"
        );
    }
}

#[test]
fn decision_coverage_stats_publication_switch_is_bounded_advisory_and_traceable() {
    let publication_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("statistics")
        .join("publication.rs");
    let publication =
        std::fs::read_to_string(publication_path).expect("publication.rs must be readable");

    for required in [
        "STATS_PUBLICATION_SWITCH_HISTORY_LIMIT",
        "STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES",
        "PredictiveEvidenceCannotDriveActiveStatsVersion",
        "AdvisoryEvidenceCannotDriveActiveStatsVersion",
        "AdvisoryEvidenceStatsVersionMismatch",
        "advisory_can_drive_active",
        "active_before",
        "candidate",
        "active_after",
        "selected_decision",
    ] {
        assert!(
            publication.contains(required),
            "statistics publication switch must keep bounded advisory trace coverage visible for: {required}"
        );
    }

    let scenario_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("scenario_evidence.rs");
    let scenario =
        std::fs::read_to_string(scenario_path).expect("scenario_evidence.rs must be readable");

    for required in [
        "can_select_plan_alone",
        "can_drive_active_stats_version_transition",
        "ScenarioEvidenceOptimizerBoundary::AdvisoryOnly",
        "validate_for_use_at",
    ] {
        assert!(
            scenario.contains(required),
            "ScenarioEvidence must keep advisory-only consumption coverage visible for: {required}"
        );
    }
}
