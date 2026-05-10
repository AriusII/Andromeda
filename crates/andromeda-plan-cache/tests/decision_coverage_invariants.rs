fn advisory_spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("specifications")
        .join("SPEC_ADVISORY_OPTIMIZER_HARDWARE_V0.md")
}

fn crate_source_path(crate_name: &str, path_segments: &[&str]) -> std::path::PathBuf {
    let mut path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_name);

    for segment in path_segments {
        path.push(segment);
    }

    path
}

fn read_crate_sources(crate_name: &str, source_paths: &[&[&str]]) -> String {
    let mut combined = String::new();

    for source_path in source_paths {
        let path = crate_source_path(crate_name, source_path);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{} must be readable: {err}", path.display()));
        combined.push_str(&source);
        combined.push('\n');
    }

    combined
}

#[test]
fn decision_coverage_plan_cache_runtime_gate_is_bounded_versioned_and_traceable() {
    let spec =
        std::fs::read_to_string(advisory_spec_path()).expect("advisory spec must be readable");
    let spec_normalized = spec
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    for required in [
        "`PlanCacheKey v0` is exactly this identity tuple",
        "`ProcedureId`",
        "`ContractHash`",
        "`CatalogVersion`",
        "`StatsVersion`",
        "`PolicyVersion`",
        "`PlanClass`",
        "`PlanShapeFingerprint`",
        "Cache hits require exact key equality and matching key digest.",
        "Advisory evidence count | Reject more than 8 scenario evidence records.",
    ] {
        assert!(
            spec_normalized.contains(&required.to_lowercase()),
            "SPEC_ADVISORY_OPTIMIZER_HARDWARE_V0.md must cover minimal PlanCache gate requirement: {required}"
        );
    }

    let plan_cache = read_crate_sources(
        "andromeda-plan-cache",
        &[
            &["src", "lib.rs"],
            &["src", "limits.rs"],
            &["src", "identity.rs"],
            &["src", "decision.rs"],
            &["src", "selection.rs"],
            &["src", "admission.rs"],
            &["src", "advisory_evidence.rs"],
        ],
    ) + &read_crate_sources(
        "andromeda-scenario-evidence",
        &[&["src", "plan_cache_bridge", "advisory_evidence.rs"]],
    );

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
            "plan-cache owner sources must keep runtime gate coverage visible for: {required}"
        );
    }
}
