use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use super::errors::BenchmarkScenarioEvidenceError;

/// Benchmark-local mirror of the catalog `StatsVersion` scalar.
///
/// The bench crate intentionally avoids depending on `andromeda-catalog`.
/// The integration layer must convert this value into the catalog type without
/// changing the raw number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BenchmarkStatsVersion(u64);

impl BenchmarkStatsVersion {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl From<u64> for BenchmarkStatsVersion {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

/// Benchmark-local mirror of the catalog bounded `PlanClass` taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BenchmarkPlanClass {
    Singleton,
    ParameterShape,
    Cardinality,
    StatsAdaptive,
}

impl BenchmarkPlanClass {
    pub const VARIANT_COUNT: usize = 4;

    pub const fn as_tag(self) -> u8 {
        match self {
            Self::Singleton => 0x01,
            Self::ParameterShape => 0x02,
            Self::Cardinality => 0x03,
            Self::StatsAdaptive => 0x04,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Singleton => "singleton",
            Self::ParameterShape => "parameter-shape",
            Self::Cardinality => "cardinality",
            Self::StatsAdaptive => "stats-adaptive",
        }
    }
}

/// Explicit target for benchmark-derived optimizer evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BenchmarkScenarioTarget {
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub contract_hash: ContractHash,
    pub stats_version: BenchmarkStatsVersion,
    pub plan_class: BenchmarkPlanClass,
}

impl BenchmarkScenarioTarget {
    pub fn new(
        procedure_id: ProcedureId,
        catalog_version: CatalogVersion,
        contract_hash: ContractHash,
        stats_version: BenchmarkStatsVersion,
        plan_class: BenchmarkPlanClass,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        let target = Self {
            procedure_id,
            catalog_version,
            contract_hash,
            stats_version,
            plan_class,
        };
        target.validate()?;
        Ok(target)
    }

    pub fn validate(self) -> Result<(), BenchmarkScenarioEvidenceError> {
        if self.procedure_id.get() == 0 {
            return Err(BenchmarkScenarioEvidenceError::TargetProcedureIdZero);
        }
        if self.catalog_version.get() == 0 {
            return Err(BenchmarkScenarioEvidenceError::TargetCatalogVersionZero);
        }
        if self.contract_hash.is_zero() {
            return Err(BenchmarkScenarioEvidenceError::TargetContractHashZero);
        }
        if self.stats_version.is_zero() {
            return Err(BenchmarkScenarioEvidenceError::TargetStatsVersionZero);
        }
        Ok(())
    }
}
