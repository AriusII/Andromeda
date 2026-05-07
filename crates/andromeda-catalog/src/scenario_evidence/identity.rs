use andromeda_core::{CatalogVersion, ContractHash, ProcedureId};

use crate::contracts::StatsVersion;
use crate::plan_cache::PlanClass;

use super::ScenarioEvidenceError;

/// Bounded scenario-kind taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScenarioKind {
    /// A bounded micro-benchmark targeting a single procedure under a fixed
    /// parameter shape.
    Microbenchmark,
    /// A representative procedure-level workload run, multiple invocations of
    /// one procedure.
    ProcedureWorkload,
    /// A regression probe used to detect performance drift between known-good
    /// baselines.
    RegressionProbe,
    /// A synthetic load designed to exercise a particular plan class.
    SyntheticLoad,
}

impl ScenarioKind {
    /// Stable tag byte folded into the evidence digest. Reordering or reusing
    /// tag bytes is a doctrine change.
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
///
/// Couples a procedure id with the catalog and statistics snapshots the
/// evidence was gathered against, plus optional plan-class and contract-hash
/// refinements. Optionality is encoded with explicit `Option` so the digest
/// distinguishes "absent" from "present-but-zero".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScenarioTarget {
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub stats_version: StatsVersion,
    pub plan_class: Option<PlanClass>,
    pub contract_hash: Option<ContractHash>,
}

impl ScenarioTarget {
    /// Validate the target. `procedure_id`, `catalog_version`, and
    /// `stats_version` must be non-zero so evidence cannot be implicitly
    /// addressed at a "no procedure" / "no catalog" / "no stats" sentinel.
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
