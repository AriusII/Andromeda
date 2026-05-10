use andromeda_procedure_contract::StatsVersion;
use andromeda_statistics::{
    CorrelationEvidenceBounds, CorrelationStrengthPermille, StatsColumnTarget, StatsCorrelation,
    StatsCorrelationId, StatsCorrelationKind, StatsCorrelationPublicationBuilder,
};
use andromeda_types::{CatalogObjectId, CatalogVersion};

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

fn map_stats_target(object_id: u64, column_index: u16) -> StatsColumnTarget {
    StatsColumnTarget::new(CatalogObjectId::new(object_id), column_index)
}

fn map_stats_correlation(id: u64, catalog_version: u64, stats_version: u64) -> StatsCorrelation {
    StatsCorrelation::new(
        StatsCorrelationId::new(id).expect("test correlation id must be non-zero"),
        CatalogVersion::new(catalog_version),
        StatsVersion::new(stats_version),
        StatsCorrelationKind::FunctionalDependency,
        CorrelationStrengthPermille::from_permille(900)
            .expect("test correlation strength must be in range"),
        vec![map_stats_target(10, 1), map_stats_target(20, 2)],
        CorrelationEvidenceBounds {
            sample_rows: 100,
            population_lower_bound: 100,
            population_upper_bound: 1_000,
            confidence_permille: 950,
        },
    )
    .expect("test correlation must be valid")
}

#[test]
fn decision_coverage_stats_publication_switch_is_bounded_advisory_and_traceable() {
    let publication = read_crate_sources(
        "andromeda-statistics",
        &[
            &["src", "publication.rs"],
            &["src", "publication_evidence.rs"],
            &["src", "publication_switch.rs"],
            &["src", "publication_switch_error.rs"],
            &["src", "publication_trace.rs"],
        ],
    );

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

    let scenario = read_crate_sources(
        "andromeda-scenario-evidence",
        &[
            &["src", "scenario_evidence", "advisory.rs"],
            &["src", "scenario_evidence", "evidence.rs"],
        ],
    );

    for required in [
        "can_select_plan_alone",
        "can_drive_active_stats_version_transition",
        "ScenarioEvidenceOptimizerBoundary::AdvisoryOnly",
        "validate_for_use_at",
    ] {
        assert!(
            scenario.contains(required),
            "ScenarioEvidence owner sources must keep advisory-only consumption coverage visible for: {required}"
        );
    }
}

#[test]
fn map_refresh_validation_spec_covers_stats_staleness_summarizability_and_truth_boundary() {
    let spec =
        std::fs::read_to_string(advisory_spec_path()).expect("advisory spec must be readable");
    let normalized = spec
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    for required in [
        "# Advisory Optimizer And Hardware",
        "## Map refresh validation",
        "## Purpose",
        "## Validation gates",
        "current catalog stats",
        "`StatsVersion`",
        "`CatalogVersion`",
        "staleness",
        "summarizability",
        "durable publication",
        "analytics treated as truth",
        "missing decision trace",
        "Map tests must prove grain, summarizability, staleness, durable publication",
    ] {
        assert!(
            normalized.contains(&required.to_lowercase()),
            "SPEC_ADVISORY_OPTIMIZER_HARDWARE_V0.md must cover required Map analytics validation topic: {required}"
        );
    }
}

#[test]
fn stats_correlation_publication_surface_keeps_map_analytics_advisory_boundaries() {
    let source = read_crate_sources(
        "andromeda-statistics",
        &[&["src", "correlation_publication.rs"]],
    );

    for required in [
        "Map analytics validators",
        "staleness",
        "summarizability",
        "durable publication",
        "never source truth",
        "is_authoritative",
        "requires_durable_publication_evidence",
        "is_current_for",
        "is_stale_for",
    ] {
        assert!(
            source.contains(required),
            "correlation publication source must preserve advisory boundary text: {required}"
        );
    }
}

#[test]
fn stats_correlation_publication_rejects_stale_map_analytics_scope() {
    let publication =
        StatsCorrelationPublicationBuilder::new(CatalogVersion::new(7), StatsVersion::new(3))
            .expect("builder versions must be valid")
            .push(map_stats_correlation(1, 7, 3))
            .expect("correlation must match publication versions")
            .finish();

    assert!(!publication.is_authoritative());
    assert!(publication.requires_durable_publication_evidence());
    assert!(publication.is_current_for(CatalogVersion::new(7), StatsVersion::new(3),));
    assert!(!publication.is_stale_for(CatalogVersion::new(7), StatsVersion::new(3),));

    assert!(!publication.is_current_for(CatalogVersion::new(8), StatsVersion::new(3),));
    assert!(publication.is_stale_for(CatalogVersion::new(7), StatsVersion::new(4),));
}
