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
    /// Eliminate dead columns from `ReadTable` row-sets.
    ProjectionPushdown,
    /// Compute `CostEstimate` for the optimized IR.
    CostAnalysis,
    /// Select the minimum-cost plan and record the decision.
    PlanChoice,
}

impl OptimizerPhase {
    /// Total number of phases. Asserted by tests to flag accidental growth.
    pub const PHASE_COUNT: usize = 8;

    /// Stable ordinal for ordering checks. Must not be reordered.
    pub const fn as_ordinal(self) -> u8 {
        match self {
            Self::Parsing => 0,
            Self::Binding => 1,
            Self::IRLowering => 2,
            Self::ConstantFolding => 3,
            Self::PredicatePushdown => 4,
            Self::ProjectionPushdown => 5,
            Self::CostAnalysis => 6,
            Self::PlanChoice => 7,
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
    fn phase_count_is_eight() {
        assert_eq!(OptimizerPhase::PHASE_COUNT, 8);
    }

    #[test]
    fn phases_are_strictly_ordered() {
        use OptimizerPhase::*;
        let all = [
            Parsing,
            Binding,
            IRLowering,
            ConstantFolding,
            PredicatePushdown,
            ProjectionPushdown,
            CostAnalysis,
            PlanChoice,
        ];
        for window in all.windows(2) {
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
