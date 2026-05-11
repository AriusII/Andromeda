/// Maximum bytes allowed in a plan-kind label recorded in cost breakdown evidence.
pub const PLAN_KIND_LABEL_MAX_BYTES: usize = 64;

/// Maximum number of plan alternatives that may be recorded in a single DecisionTrace.
pub const COST_BREAKDOWN_ALTERNATIVE_LIMIT: usize = 32;

use crate::DecisionTraceError;

/// Cost components for a single plan alternative, mirroring the optimizer's
/// seven-component model.  Defined here (not in andromeda-optimizer) so that
/// andromeda-decision-trace remains free of any upward dependency.
///
/// All components must be finite and non-negative; total must equal the
/// sum of the seven components.  Callers are responsible for ensuring
/// consistency before constructing [`PlanAlternativeEvidence`].
#[derive(Debug, Clone, PartialEq)]
pub struct PlanAlternativeCost {
    pub cpu_cost: f64,
    pub logical_io_cost: f64,
    pub physical_io_cost: f64,
    pub wal_cost: f64,
    pub temp_cost: f64,
    pub network_cost: f64,
    pub risk_penalty_cost: f64,
    pub total_cost: f64,
}

impl PlanAlternativeCost {
    /// A zero-cost instance used as a neutral baseline.
    pub const fn zero() -> Self {
        Self {
            cpu_cost: 0.0,
            logical_io_cost: 0.0,
            physical_io_cost: 0.0,
            wal_cost: 0.0,
            temp_cost: 0.0,
            network_cost: 0.0,
            risk_penalty_cost: 0.0,
            total_cost: 0.0,
        }
    }
}

/// Reason a plan alternative was not chosen, as recorded in a `DecisionTrace`.
///
/// This enum mirrors `andromeda_optimizer::srpl::plan_choice::RejectionReason`
/// but is intentionally re-declared here so that `andromeda-decision-trace`
/// does not depend on `andromeda-optimizer`.  Conversion happens at the
/// optimizer boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanRejectionReason {
    /// The alternative had a strictly higher total cost than the chosen plan.
    HigherCost,
    /// The alternative tied on cost but had a lower-priority plan class.
    TiedCostLowerPriority,
    /// The alternative carried invalid (NaN / infinite / negative) cost evidence.
    InvalidCostEvidence,
}

impl PlanRejectionReason {
    /// Stable machine-readable label for this rejection reason.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HigherCost => "higher-cost",
            Self::TiedCostLowerPriority => "tied-cost-lower-priority",
            Self::InvalidCostEvidence => "invalid-cost-evidence",
        }
    }
}

/// Advisory evidence of columnar chunk pruning, carried by a
/// [`DecisionTrace`][crate::DecisionTrace].
///
/// Records how many chunks were skipped (outside the predicate range) versus
/// the total evaluated.  Advisory only — pruning must never decide correctness;
/// ambiguous or metadata-absent chunks are always scanned.
///
/// Declared in `andromeda-decision-trace` (not in `andromeda-optimizer` or
/// `andromeda-columnar`) so that this crate remains free of upward dependencies.
/// Boundary conversion happens at the optimizer layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnarPruningEvidence {
    /// Chunks whose min/max range was disjoint from the predicate and were skipped.
    pub skipped: usize,
    /// Total chunks evaluated for this column.
    pub total: usize,
}

impl ColumnarPruningEvidence {
    /// Chunks that must still be scanned (`total - skipped`).
    pub const fn scannable(self) -> usize {
        self.total.saturating_sub(self.skipped)
    }

    /// Returns `true` when at least one chunk was pruned.
    pub const fn any_pruned(self) -> bool {
        self.skipped > 0
    }
}

/// Per-alternative evidence record carried by a [`DecisionTrace`][crate::DecisionTrace].
///
/// Holds the plan-kind label, full cost breakdown, whether this alternative
/// was chosen, and — if not — the reason it was rejected.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanAlternativeEvidence {
    plan_kind: String,
    cost: PlanAlternativeCost,
    chosen: bool,
    rejection: Option<PlanRejectionReason>,
}

impl PlanAlternativeEvidence {
    /// Construct a new evidence record.
    ///
    /// # Errors
    /// - [`DecisionTraceError::PlanKindLabelEmpty`] — `plan_kind` is blank.
    /// - [`DecisionTraceError::PlanKindLabelTooLong`] — `plan_kind` exceeds
    ///   [`PLAN_KIND_LABEL_MAX_BYTES`].
    pub fn new(
        plan_kind: impl Into<String>,
        cost: PlanAlternativeCost,
        chosen: bool,
        rejection: Option<PlanRejectionReason>,
    ) -> Result<Self, DecisionTraceError> {
        let plan_kind = plan_kind.into();
        let trimmed = plan_kind.trim();
        if trimmed.is_empty() {
            return Err(DecisionTraceError::PlanKindLabelEmpty);
        }
        if trimmed.len() > PLAN_KIND_LABEL_MAX_BYTES {
            return Err(DecisionTraceError::PlanKindLabelTooLong);
        }
        Ok(Self {
            plan_kind: trimmed.to_string(),
            cost,
            chosen,
            rejection,
        })
    }

    /// Plan-kind label (e.g. `"point-lookup"`, `"range-scan"`).
    pub fn plan_kind(&self) -> &str {
        &self.plan_kind
    }

    /// Cost breakdown for this alternative.
    pub fn cost(&self) -> &PlanAlternativeCost {
        &self.cost
    }

    /// Whether this alternative was the one selected by the optimizer.
    pub fn chosen(&self) -> bool {
        self.chosen
    }

    /// Rejection reason, present when `chosen` is `false`.
    pub fn rejection(&self) -> Option<PlanRejectionReason> {
        self.rejection
    }
}
