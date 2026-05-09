use andromeda_plan_cache::PlanClass;
use andromeda_procedure_contract::StatsVersion;
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use super::ScenarioEvidenceError;

/// Bounded scenario-kind taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScenarioKind {
    /// A bounded micro-benchmark targeting a single procedure under a fixed
    /// parameter shape.
    Microbenchmark,
    /// A representative procedure-level workload run.
    ProcedureWorkload,
    /// A regression probe used to detect performance drift.
    RegressionProbe,
    /// A synthetic load designed to exercise a particular plan class.
    SyntheticLoad,
}

impl ScenarioKind {
    pub const fn as_tag(self) -> u8 {
        match self {
            ScenarioKind::Microbenchmark => 0x01,
            ScenarioKind::ProcedureWorkload => 0x02,
            ScenarioKind::RegressionProbe => 0x03,
            ScenarioKind::SyntheticLoad => 0x04,
        }
    }

    pub const VARIANT_COUNT: usize = 4;
}

/// Stable scenario identity. Non-zero so a "no scenario" sentinel cannot
/// accidentally produce a valid digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScenarioId(u64);

impl ScenarioId {
    /// Construct a `ScenarioId`. Returns `None` for the reserved zero value.
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Bounded targeting metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScenarioTarget {
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub stats_version: StatsVersion,
    pub plan_class: Option<PlanClass>,
    pub contract_hash: Option<ContractHash>,
}

impl ScenarioTarget {
    /// Validate the target. Required identifiers must be non-zero so evidence
    /// cannot be implicitly addressed at sentinel values.
    pub fn validate(&self) -> Result<(), ScenarioEvidenceError> {
        if self.procedure_id.get() == 0 {
            return Err(ScenarioEvidenceError::TargetProcedureIdZero);
        }
        if self.catalog_version.get() == 0 {
            return Err(ScenarioEvidenceError::TargetCatalogVersionZero);
        }
        if self.stats_version.get() == 0 {
            return Err(ScenarioEvidenceError::TargetStatsVersionZero);
        }
        if let Some(hash) = self.contract_hash
            && hash.is_zero()
        {
            return Err(ScenarioEvidenceError::TargetContractHashZero);
        }
        Ok(())
    }
}
