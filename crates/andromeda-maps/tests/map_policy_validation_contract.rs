use andromeda_maps::{
    MapDependency, MapDependencyGraph, MapDescriptor, MapGrain, MapId, MapMeasurePolicy,
    MapMeasureRollupPolicy, MapRefreshMode, MapStalenessPolicy, MapSummarizabilityMode,
    MapSummarizabilityPolicy, MapValidationDiagnostic,
};

fn map_id(value: u64) -> MapId {
    match MapId::new(value) {
        Ok(id) => id,
        Err(error) => panic!("unexpected map id error: {error}"),
    }
}

fn descriptor(refresh_mode: MapRefreshMode) -> MapDescriptor {
    MapDescriptor::new(
        map_id(7),
        MapGrain::Relation,
        refresh_mode,
        MapStalenessPolicy::CurrentOnly,
    )
}

#[test]
fn refresh_modes_use_roadmap_names() {
    let modes = [
        MapRefreshMode::Immediate,
        MapRefreshMode::Incremental,
        MapRefreshMode::Deferred,
        MapRefreshMode::SnapshotOnly,
    ];

    assert_eq!(modes.len(), 4);
}

#[test]
fn dependency_graph_rejects_missing_dependency() {
    let graph = MapDependencyGraph::new(
        vec![map_id(1), map_id(2)],
        vec![MapDependency::new(map_id(1), map_id(3))],
    );

    let report = graph.validate();

    assert_eq!(
        report.diagnostics(),
        &[MapValidationDiagnostic::MissingDependency {
            map_id: map_id(1),
            missing_dependency: map_id(3),
        }]
    );
    assert!(!report.is_valid());
}

#[test]
fn dependency_graph_rejects_cycle() {
    let graph = MapDependencyGraph::new(
        vec![map_id(1), map_id(2), map_id(3)],
        vec![
            MapDependency::new(map_id(1), map_id(2)),
            MapDependency::new(map_id(2), map_id(3)),
            MapDependency::new(map_id(3), map_id(1)),
        ],
    );

    let report = graph.validate();

    assert_eq!(
        report.diagnostics(),
        &[MapValidationDiagnostic::DependencyCycle {
            cycle: vec![map_id(1), map_id(2), map_id(3), map_id(1)],
        }]
    );
    assert!(!report.is_valid());
}

#[test]
fn dependency_graph_accepts_acyclic_declared_dependencies() {
    let graph = MapDependencyGraph::new(
        vec![map_id(1), map_id(2), map_id(3)],
        vec![
            MapDependency::new(map_id(1), map_id(2)),
            MapDependency::new(map_id(1), map_id(3)),
            MapDependency::new(map_id(2), map_id(3)),
        ],
    );

    let report = graph.validate();

    assert!(report.is_valid());
    assert!(report.diagnostics().is_empty());
}

#[test]
fn rollup_policy_rejects_missing_grouping_key() {
    let policy = MapSummarizabilityPolicy::new(
        descriptor(MapRefreshMode::Incremental),
        MapSummarizabilityMode::RollupAllowed,
        0,
        vec![MapMeasurePolicy::new(0, MapMeasureRollupPolicy::Additive)],
    );

    let report = policy.validate();

    assert_eq!(
        report.diagnostics(),
        &[MapValidationDiagnostic::MissingSummarizabilityGroupingKey { map_id: map_id(7) }]
    );
    assert!(!report.is_valid());
    assert!(!policy.is_source_truth());
}

#[test]
fn rollup_policy_rejects_non_summarizable_measure() {
    let policy = MapSummarizabilityPolicy::new(
        descriptor(MapRefreshMode::Deferred),
        MapSummarizabilityMode::RollupAllowed,
        1,
        vec![MapMeasurePolicy::new(
            2,
            MapMeasureRollupPolicy::NonAdditive,
        )],
    );

    let report = policy.validate();

    assert_eq!(
        report.diagnostics(),
        &[MapValidationDiagnostic::NonSummarizableMeasure {
            map_id: map_id(7),
            measure_ordinal: 2,
            rollup_policy: MapMeasureRollupPolicy::NonAdditive,
        }]
    );
    assert!(!report.is_valid());
}

#[test]
fn rollup_policy_accepts_additive_and_idempotent_measures() {
    let policy = MapSummarizabilityPolicy::new(
        descriptor(MapRefreshMode::SnapshotOnly),
        MapSummarizabilityMode::RollupAllowed,
        2,
        vec![
            MapMeasurePolicy::new(0, MapMeasureRollupPolicy::Additive),
            MapMeasurePolicy::new(1, MapMeasureRollupPolicy::Idempotent),
        ],
    );

    let report = policy.validate();

    assert!(report.is_valid());
    assert_eq!(policy.measures().len(), 2);
    assert!(!policy.is_source_truth());
}
