use crate::{MapDescriptor, MapValidationDiagnostic, MapValidationReport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapSummarizabilityMode {
    ExactGrainOnly,
    RollupAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapMeasureRollupPolicy {
    Additive,
    Idempotent,
    SemiAdditive,
    NonAdditive,
}

impl MapMeasureRollupPolicy {
    pub const fn is_rollup_safe(self) -> bool {
        matches!(self, Self::Additive | Self::Idempotent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapMeasurePolicy {
    pub measure_ordinal: u16,
    pub rollup_policy: MapMeasureRollupPolicy,
}

impl MapMeasurePolicy {
    pub const fn new(measure_ordinal: u16, rollup_policy: MapMeasureRollupPolicy) -> Self {
        Self {
            measure_ordinal,
            rollup_policy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSummarizabilityPolicy {
    pub descriptor: MapDescriptor,
    pub mode: MapSummarizabilityMode,
    pub grouping_key_count: u16,
    measures: Vec<MapMeasurePolicy>,
}

impl MapSummarizabilityPolicy {
    pub fn new(
        descriptor: MapDescriptor,
        mode: MapSummarizabilityMode,
        grouping_key_count: u16,
        measures: Vec<MapMeasurePolicy>,
    ) -> Self {
        Self {
            descriptor,
            mode,
            grouping_key_count,
            measures,
        }
    }

    pub fn measures(&self) -> &[MapMeasurePolicy] {
        &self.measures
    }

    pub fn validate(&self) -> MapValidationReport {
        let mut report = MapValidationReport::new();

        if self.mode == MapSummarizabilityMode::RollupAllowed {
            self.validate_rollup_allowed(&mut report);
        }

        report
    }

    pub fn is_valid(&self) -> bool {
        self.validate().is_valid()
    }

    pub const fn is_source_truth(&self) -> bool {
        false
    }

    fn validate_rollup_allowed(&self, report: &mut MapValidationReport) {
        if self.grouping_key_count == 0 {
            report.push(MapValidationDiagnostic::MissingSummarizabilityGroupingKey {
                map_id: self.descriptor.id,
            });
        }

        if self.measures.is_empty() {
            report.push(MapValidationDiagnostic::MissingSummarizableMeasure {
                map_id: self.descriptor.id,
            });
        }

        for measure in &self.measures {
            if !measure.rollup_policy.is_rollup_safe() {
                report.push(MapValidationDiagnostic::NonSummarizableMeasure {
                    map_id: self.descriptor.id,
                    measure_ordinal: measure.measure_ordinal,
                    rollup_policy: measure.rollup_policy,
                });
            }
        }
    }
}
