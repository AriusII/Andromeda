/// Maximum number of plan candidates that the minimal selector may consider.
///
/// This keeps the V0 gate deterministic and prevents accidental unbounded
/// plan-class explosion before a dedicated optimizer crate exists.
pub const PLAN_SELECTION_MAX_CANDIDATES: usize = 8;

/// Maximum number of advisory `ScenarioEvidence` records consumed by one
/// selection decision.
pub const PLAN_SELECTION_MAX_SCENARIO_EVIDENCE: usize = 8;

/// Maximum number of entries in the bounded in-memory PlanCache gate.
pub const PLAN_CACHE_MAX_ENTRIES: usize = 64;
