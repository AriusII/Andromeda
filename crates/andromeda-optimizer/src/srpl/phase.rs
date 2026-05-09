/// Optimizer pipeline phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OptimizerPhase {
    /// Parse SRPL source text into an unresolved `ProcedureAst`.
    Parsing,
    /// Bind AST symbols to catalog objects; validate type descriptors.
    Binding,
    /// Lower `BoundProcedure` to `SrplProcedureIr`.
    IRLowering,
    /// Fold constant expressions, predicates, and pure functions.
    ConstantFolding,
    /// Push scalar predicates toward their upstream `ReadTable` scan.
    PredicatePushdown,
    /// Canonicalize predicates and ordinals after structural rewrites.
    Normalize,
    /// Eliminate dead columns from `ReadTable` row-sets.
    ProjectionPushdown,
    /// Compute `CostEstimate` for the optimized IR.
    CostAnalysis,
    /// Select the minimum-cost plan and record the decision.
    PlanChoice,
}

impl OptimizerPhase {
    /// Total number of phases. Asserted by tests to flag accidental growth.
    pub const PHASE_COUNT: usize = 9;

    /// Canonical phase order. Must not be reordered without a pipeline doctrine update.
    pub const ORDERED: [Self; Self::PHASE_COUNT] = [
        Self::Parsing,
        Self::Binding,
        Self::IRLowering,
        Self::ConstantFolding,
        Self::PredicatePushdown,
        Self::Normalize,
        Self::ProjectionPushdown,
        Self::CostAnalysis,
        Self::PlanChoice,
    ];

    /// Rewrites controlled by `OptimizationLevel`.
    pub const REWRITE_PHASES: [Self; 3] = [
        Self::ConstantFolding,
        Self::PredicatePushdown,
        Self::Normalize,
    ];

    /// Stable ordinal for ordering checks. Must not be reordered.
    pub const fn as_ordinal(self) -> u8 {
        match self {
            Self::Parsing => 0,
            Self::Binding => 1,
            Self::IRLowering => 2,
            Self::ConstantFolding => 3,
            Self::PredicatePushdown => 4,
            Self::Normalize => 5,
            Self::ProjectionPushdown => 6,
            Self::CostAnalysis => 7,
            Self::PlanChoice => 8,
        }
    }

    /// True when `other` is the valid immediate predecessor in the pipeline.
    pub const fn requires_phase(self, other: OptimizerPhase) -> bool {
        other.as_ordinal() + 1 == self.as_ordinal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_count_is_nine() {
        assert_eq!(OptimizerPhase::PHASE_COUNT, 9);
    }

    #[test]
    fn phases_are_strictly_ordered() {
        for window in OptimizerPhase::ORDERED.windows(2) {
            assert!(
                window[0].as_ordinal() < window[1].as_ordinal(),
                "phase ordering violated: {:?} must precede {:?}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn requires_phase_reflects_predecessor() {
        assert!(OptimizerPhase::ConstantFolding.requires_phase(OptimizerPhase::IRLowering));
        assert!(OptimizerPhase::PlanChoice.requires_phase(OptimizerPhase::CostAnalysis));
        assert!(!OptimizerPhase::PlanChoice.requires_phase(OptimizerPhase::IRLowering));
    }
}
