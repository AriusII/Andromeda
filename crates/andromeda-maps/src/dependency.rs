use std::collections::{BTreeMap, BTreeSet};

use crate::{MapId, MapValidationDiagnostic, MapValidationReport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapDependency {
    pub map_id: MapId,
    pub depends_on: MapId,
}

impl MapDependency {
    pub const fn new(map_id: MapId, depends_on: MapId) -> Self {
        Self { map_id, depends_on }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapDependencyGraph {
    maps: Vec<MapId>,
    dependencies: Vec<MapDependency>,
}

impl MapDependencyGraph {
    pub fn new(maps: Vec<MapId>, dependencies: Vec<MapDependency>) -> Self {
        Self { maps, dependencies }
    }

    pub fn maps(&self) -> &[MapId] {
        &self.maps
    }

    pub fn dependencies(&self) -> &[MapDependency] {
        &self.dependencies
    }

    pub fn validate(&self) -> MapValidationReport {
        let mut report = MapValidationReport::new();
        let mut declared = BTreeSet::new();
        let mut adjacency = BTreeMap::<MapId, BTreeSet<MapId>>::new();

        for map_id in self.maps.iter().copied() {
            if !declared.insert(map_id) {
                report.push(MapValidationDiagnostic::DuplicateMap { map_id });
            }
            adjacency.entry(map_id).or_default();
        }

        for dependency in self.dependencies.iter().copied() {
            let source_is_declared = declared.contains(&dependency.map_id);
            let target_is_declared = declared.contains(&dependency.depends_on);

            if !source_is_declared {
                report.push(MapValidationDiagnostic::DependencySourceNotDeclared {
                    map_id: dependency.map_id,
                });
            }

            if !target_is_declared {
                report.push(MapValidationDiagnostic::MissingDependency {
                    map_id: dependency.map_id,
                    missing_dependency: dependency.depends_on,
                });
            }

            if source_is_declared && target_is_declared {
                adjacency
                    .entry(dependency.map_id)
                    .or_default()
                    .insert(dependency.depends_on);
            }
        }

        let adjacency = adjacency
            .into_iter()
            .map(|(map_id, dependencies)| {
                (map_id, dependencies.into_iter().collect::<Vec<MapId>>())
            })
            .collect::<BTreeMap<MapId, Vec<MapId>>>();

        detect_cycles(&adjacency, &mut report);

        report
    }

    pub fn is_valid(&self) -> bool {
        self.validate().is_valid()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Visited,
}

fn detect_cycles(adjacency: &BTreeMap<MapId, Vec<MapId>>, report: &mut MapValidationReport) {
    let mut states = BTreeMap::new();
    let mut stack = Vec::new();

    for map_id in adjacency.keys().copied() {
        if !states.contains_key(&map_id) {
            visit(map_id, adjacency, &mut states, &mut stack, report);
        }
    }
}

fn visit(
    map_id: MapId,
    adjacency: &BTreeMap<MapId, Vec<MapId>>,
    states: &mut BTreeMap<MapId, VisitState>,
    stack: &mut Vec<MapId>,
    report: &mut MapValidationReport,
) {
    states.insert(map_id, VisitState::Visiting);
    stack.push(map_id);

    if let Some(dependencies) = adjacency.get(&map_id) {
        for dependency in dependencies.iter().copied() {
            match states.get(&dependency).copied() {
                None => visit(dependency, adjacency, states, stack, report),
                Some(VisitState::Visiting) => {
                    if let Some(cycle_start) = stack.iter().position(|id| *id == dependency) {
                        let mut cycle = stack[cycle_start..].to_vec();
                        cycle.push(dependency);
                        report.push(MapValidationDiagnostic::DependencyCycle { cycle });
                    }
                }
                Some(VisitState::Visited) => {}
            }
        }
    }

    stack.pop();
    states.insert(map_id, VisitState::Visited);
}
