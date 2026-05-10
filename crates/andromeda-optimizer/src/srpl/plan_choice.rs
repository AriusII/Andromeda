use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_srpl_ir::SrplProcedureIr;

use super::cost_model::CostEstimate;
use super::plan_kind::OptimizerPlanKind;

/// Record of a non-chosen alternative, written to the Procedure Store.
#[derive(Debug, Clone)]
pub struct AlternativePlanRecord {
    pub plan_kind: OptimizerPlanKind,
    pub cost_estimate: CostEstimate,
    pub chosen: bool,
    pub rejection: Option<RejectionReason>,
}

/// Reason a plan alternative was not chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    HigherCost,
    TiedCostLowerPriority,
    InvalidCostEvidence,
}

/// The result of `PlanChoice::choose`.
#[derive(Debug)]
pub struct PlanChoiceResult {
    /// The selected plan IR.
    pub chosen_ir: SrplProcedureIr,
    /// Cost estimate of the chosen plan.
    pub chosen_cost: CostEstimate,
    /// Plan kind of the chosen plan.
    pub chosen_kind: OptimizerPlanKind,
    /// All alternatives (including the chosen one, marked `chosen: true`).
    pub all_alternatives: Vec<AlternativePlanRecord>,
}

/// Select the minimum-cost plan from a set of alternatives.
///
/// Ties are broken by `OptimizerPlanKind::priority` (lower = simpler).
///
/// # Errors
/// Returns an error when `alternatives` is empty.
pub fn choose(
    alternatives: Vec<(SrplProcedureIr, CostEstimate, OptimizerPlanKind)>,
) -> AndromedaResult<PlanChoiceResult> {
    if alternatives.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "optimizer plan choice received zero alternatives",
        ));
    }

    for (_, cost, kind) in &alternatives {
        if !cost.is_valid() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!("optimizer plan choice rejected invalid cost evidence for {kind:?}"),
            ));
        }
    }

    // Find the index of the minimum-cost / lowest-priority-tie alternative.
    let chosen_idx = alternatives
        .iter()
        .enumerate()
        .min_by(|(_, (_, cost_a, kind_a)), (_, (_, cost_b, kind_b))| {
            cost_a
                .total_cost
                .total_cmp(&cost_b.total_cost)
                .then(kind_a.priority().cmp(&kind_b.priority()))
        })
        .map(|(i, _)| i)
        .expect("alternatives is non-empty; min_by must return Some");

    // Build the alternatives record list before consuming the vec.
    let mut all_records = Vec::with_capacity(alternatives.len());
    for (i, (_, cost, kind)) in alternatives.iter().enumerate() {
        let chosen = i == chosen_idx;
        let rejection = if !chosen {
            let chosen_cost = alternatives[chosen_idx].1.total_cost;
            if cost.total_cost > chosen_cost {
                Some(RejectionReason::HigherCost)
            } else {
                Some(RejectionReason::TiedCostLowerPriority)
            }
        } else {
            None
        };
        all_records.push(AlternativePlanRecord {
            plan_kind: *kind,
            cost_estimate: *cost,
            chosen,
            rejection,
        });
    }

    let (chosen_ir, chosen_cost, chosen_kind) = alternatives
        .into_iter()
        .nth(chosen_idx)
        .expect("chosen_idx is valid");

    Ok(PlanChoiceResult {
        chosen_ir,
        chosen_cost,
        chosen_kind,
        all_alternatives: all_records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_procedure_contract::QualifiedName;
    use andromeda_srpl_ir::{SrplProcedureBodyIr, SrplProcedureIr};

    fn empty_ir() -> SrplProcedureIr {
        SrplProcedureIr {
            name: QualifiedName::parse("db.ns.P").unwrap(),
            inputs: vec![],
            result_streams: vec![],
            body: SrplProcedureBodyIr { operations: vec![] },
        }
    }

    fn cost(total: f64) -> CostEstimate {
        CostEstimate {
            cpu_cost: total * 0.5,
            io_cost: total * 0.4,
            memory_cost: total * 0.1,
            total_cost: total,
        }
    }

    #[test]
    fn choose_minimum_cost_plan() {
        let alternatives = vec![
            (empty_ir(), cost(100.0), OptimizerPlanKind::RangeScan),
            (empty_ir(), cost(20.0), OptimizerPlanKind::PointLookup),
            (empty_ir(), cost(50.0), OptimizerPlanKind::SingleRowInsert),
        ];
        let result = choose(alternatives).unwrap();
        assert_eq!(result.chosen_kind, OptimizerPlanKind::PointLookup);
        assert!((result.chosen_cost.total_cost - 20.0).abs() < 0.001);
    }

    #[test]
    fn tie_broken_by_priority() {
        // Both have cost 10.0; PointLookup has lower priority (0) than RangeScan (4).
        let alternatives = vec![
            (empty_ir(), cost(10.0), OptimizerPlanKind::RangeScan),
            (empty_ir(), cost(10.0), OptimizerPlanKind::PointLookup),
        ];
        let result = choose(alternatives).unwrap();
        assert_eq!(result.chosen_kind, OptimizerPlanKind::PointLookup);
    }

    #[test]
    fn single_alternative_is_always_chosen() {
        let alternatives = vec![(empty_ir(), cost(42.0), OptimizerPlanKind::BulkInsert)];
        let result = choose(alternatives).unwrap();
        assert_eq!(result.chosen_kind, OptimizerPlanKind::BulkInsert);
        assert_eq!(result.all_alternatives.len(), 1);
        assert!(result.all_alternatives[0].chosen);
    }

    #[test]
    fn empty_alternatives_returns_error() {
        let result = choose(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_cost_evidence_returns_error() {
        let invalid_costs = [
            CostEstimate {
                total_cost: f64::NAN,
                ..CostEstimate::zero()
            },
            CostEstimate {
                cpu_cost: f64::INFINITY,
                total_cost: f64::INFINITY,
                ..CostEstimate::zero()
            },
            CostEstimate {
                cpu_cost: -1.0,
                total_cost: -1.0,
                ..CostEstimate::zero()
            },
            CostEstimate {
                cpu_cost: 1.0,
                io_cost: 1.0,
                memory_cost: 1.0,
                total_cost: 2.0,
            },
        ];

        for invalid_cost in invalid_costs {
            let result = choose(vec![(
                empty_ir(),
                invalid_cost,
                OptimizerPlanKind::PointLookup,
            )]);
            assert!(result.is_err());
        }
    }

    #[test]
    fn all_non_chosen_alternatives_have_rejection_reason() {
        let alternatives = vec![
            (empty_ir(), cost(50.0), OptimizerPlanKind::RangeScan),
            (empty_ir(), cost(10.0), OptimizerPlanKind::PointLookup),
        ];
        let result = choose(alternatives).unwrap();
        let non_chosen: Vec<_> = result
            .all_alternatives
            .iter()
            .filter(|a| !a.chosen)
            .collect();
        for alt in non_chosen {
            assert!(
                alt.rejection.is_some(),
                "non-chosen alternative must have a rejection reason"
            );
        }
    }
}
