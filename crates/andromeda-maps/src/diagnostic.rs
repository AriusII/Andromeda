use crate::{MapId, MapMeasureRollupPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapValidationDiagnostic {
    DuplicateMap {
        map_id: MapId,
    },
    DependencySourceNotDeclared {
        map_id: MapId,
    },
    MissingDependency {
        map_id: MapId,
        missing_dependency: MapId,
    },
    DependencyCycle {
        cycle: Vec<MapId>,
    },
    MissingSummarizabilityGroupingKey {
        map_id: MapId,
    },
    MissingSummarizableMeasure {
        map_id: MapId,
    },
    NonSummarizableMeasure {
        map_id: MapId,
        measure_ordinal: u16,
        rollup_policy: MapMeasureRollupPolicy,
    },
}

impl MapValidationDiagnostic {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::DuplicateMap { .. } => "MAP_DUPLICATE",
            Self::DependencySourceNotDeclared { .. } => "MAP_DEPENDENCY_SOURCE_NOT_DECLARED",
            Self::MissingDependency { .. } => "MAP_DEPENDENCY_MISSING",
            Self::DependencyCycle { .. } => "MAP_DEPENDENCY_CYCLE",
            Self::MissingSummarizabilityGroupingKey { .. } => {
                "MAP_SUMMARIZABILITY_GROUPING_KEY_MISSING"
            },
            Self::MissingSummarizableMeasure { .. } => "MAP_SUMMARIZABILITY_MEASURE_MISSING",
            Self::NonSummarizableMeasure { .. } => "MAP_SUMMARIZABILITY_MEASURE_NOT_ROLLUP_SAFE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapValidationReport {
    diagnostics: Vec<MapValidationDiagnostic>,
}

impl MapValidationReport {
    pub const fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub fn from_diagnostics(diagnostics: Vec<MapValidationDiagnostic>) -> Self {
        Self { diagnostics }
    }

    pub fn push(&mut self, diagnostic: MapValidationDiagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn extend(&mut self, other: Self) {
        self.diagnostics.extend(other.diagnostics);
    }

    pub fn is_valid(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn diagnostics(&self) -> &[MapValidationDiagnostic] {
        &self.diagnostics
    }

    pub fn into_diagnostics(self) -> Vec<MapValidationDiagnostic> {
        self.diagnostics
    }
}
