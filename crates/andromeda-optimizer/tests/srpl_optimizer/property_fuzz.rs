use proptest::prelude::*;

use super::*;

/// Property: fold_value never panics on any syntactically valid SrplValueIr.
///
/// Generates arbitrary BinaryArith trees from Int64 constants and verifies that
/// fold_value always returns Ok without panicking.
#[test]
fn t_pr_01_fold_value_never_panics_on_int64_arith() {
    proptest!(|(
        left_val in any::<i64>(),
        right_val in any::<i64>(),
        op in prop::sample::select(vec![
            ArithOp::Add,
            ArithOp::Subtract,
            ArithOp::Multiply,
            ArithOp::Divide,
        ])
    )| {
        let expr = arith(
            op,
            SrplValueIr::Constant(ConstantLiteral::Int64(left_val)),
            SrplValueIr::Constant(ConstantLiteral::Int64(right_val)),
        );
        let result = fold_value(expr);
        prop_assert!(result.is_ok(), "fold_value must return Ok for any Int64 BinaryArith");
    });
}

/// Property: fold_value is idempotent for any Int64 arith expression.
#[test]
fn t_pr_02_fold_idempotent_for_any_int64_arith() {
    proptest!(|(
        left_val in any::<i64>(),
        right_val in any::<i64>(),
        op in prop::sample::select(vec![
            ArithOp::Add,
            ArithOp::Subtract,
            ArithOp::Multiply,
            ArithOp::Divide,
        ])
    )| {
        let expr = arith(
            op,
            SrplValueIr::Constant(ConstantLiteral::Int64(left_val)),
            SrplValueIr::Constant(ConstantLiteral::Int64(right_val)),
        );
        let once = fold_value(expr).unwrap();
        let twice = fold_value(once.clone()).unwrap();
        prop_assert_eq!(once, twice, "fold must be idempotent");
    });
}

/// Property: simplify_predicates never increases predicate list length.
#[test]
fn t_pr_03_simplify_predicates_never_grows() {
    proptest!(|(count in 0usize..20)| {
        let preds: Vec<SrplPredicateIr> = (0..count)
            .map(|i| eq_pred(&format!("p{}", i % 5), "T", &format!("f{}", i % 5)))
            .collect();
        let input_len = preds.len();
        let result_len = match simplify_predicates(preds) {
            SimplifiedPredicates::Predicates(v) => v.len(),
            SimplifiedPredicates::AlwaysFalse => 0,
        };
        prop_assert!(result_len <= input_len, "simplify must not grow predicate list");
    });
}

/// Property: simplify_predicates is idempotent.
#[test]
fn t_pr_04_simplify_predicates_idempotent() {
    proptest!(|(count in 0usize..10)| {
        let preds: Vec<SrplPredicateIr> = (0..count)
            .map(|i| eq_pred(&format!("p{}", i % 3), "T", &format!("f{}", i % 3)))
            .collect();
        let once = simplified_predicates(preds);
        let twice = simplified_predicates(once.clone());
        prop_assert_eq!(once, twice, "simplify_predicates must be idempotent");
    });
}

/// Property: cost estimate is always non-negative and valid for any number
///           of Read operations with Cardinality::One.
#[test]
fn t_pr_05_cost_always_valid_and_nonnegative() {
    proptest!(|(op_count in 0usize..8)| {
        let ops: Vec<SrplBusinessOperationIr> = (0..op_count)
            .map(|i| read_op(i as u32, "T", vec![]))
            .collect();
        let ir = make_ir(ops);
        let cost_est = estimate_without_stats(&ir);
        prop_assert!(cost_est.is_valid(), "cost must be valid");
        prop_assert!(cost_est.total_cost >= 0.0, "total_cost must be non-negative");
        prop_assert!(!cost_est.total_cost.is_nan(), "total_cost must not be NaN");
    });
}

/// Property: normalize is always idempotent.
#[test]
fn t_pr_06_normalize_always_idempotent() {
    proptest!(|(pred_count in 0usize..5)| {
        let preds: Vec<SrplPredicateIr> = (0..pred_count)
            .map(|i| eq_pred(&format!("p{}", i), "T", &format!("f{}", i)))
            .collect();
        let ir = make_ir(vec![read_op(0, "T", preds)]);
        let once = normalize(ir).unwrap();
        let twice = normalize(once.clone()).unwrap();
        prop_assert_eq!(once, twice, "normalize must be idempotent");
    });
}

/// Property: pushdown_apply preserves non-zero operation count for non-pushable bodies.
#[test]
fn t_pr_07_pushdown_preserves_raise_operations() {
    proptest!(|(count in 1usize..5)| {
        let ops: Vec<SrplBusinessOperationIr> = (0..count)
            .map(|i| raise_op(i as u32))
            .collect();
        let ir = make_ir(ops);
        let result = pushdown_apply(ir).unwrap();
        prop_assert_eq!(
            result.body.operations.len(),
            count,
            "Raise operations must never be removed by pushdown"
        );
    });
}

/// Property: ordinals are always dense and zero-based after pushdown.
#[test]
fn t_pr_08_pushdown_ordinals_always_dense() {
    proptest!(|(pred_count in 0usize..4)| {
        let preds: Vec<SrplPredicateIr> = (0..pred_count)
            .map(|i| eq_pred(&format!("p{}", i), "T", &format!("f{}", i)))
            .collect();
        let mut ops = vec![read_op(0, "T", vec![])];
        for (i, pred) in preds.into_iter().enumerate() {
            ops.push(assert_op((i + 1) as u32, pred));
        }
        let ir = make_ir(ops);
        let result = pushdown_apply(ir).unwrap();
        for (pos, op) in result.body.operations.iter().enumerate() {
            prop_assert_eq!(
                op.ordinal,
                pos as u32,
                "ordinal {} must equal position {}",
                op.ordinal,
                pos
            );
        }
    });
}

/// Property: plan choice always selects a plan with cost ≤ all others.
#[test]
fn t_pr_09_plan_choice_always_selects_minimum_cost() {
    proptest!(|(costs in prop::collection::vec(0.0f64..1000.0, 1..6))| {
        let alts: Vec<(SrplProcedureIr, CostEstimate, OptimizerPlanKind)> = costs
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let kind = if i % 2 == 0 {
                    OptimizerPlanKind::RangeScan
                } else {
                    OptimizerPlanKind::PointLookup
                };
                (empty_ir(), cost(c), kind)
            })
            .collect();
        let min_cost = costs.iter().cloned().fold(f64::MAX, f64::min);
        let result = choose(alts).unwrap();
        prop_assert!(
            result.chosen_cost.total_cost <= min_cost + f64::EPSILON,
            "chosen cost {} must be ≤ minimum {}", result.chosen_cost.total_cost, min_cost
        );
    });
}

/// Property: fold_emit_values returns the same number of emit values.
#[test]
fn t_pr_10_fold_emit_values_preserves_count() {
    proptest!(|(count in 1usize..8)| {
        let values: Vec<SrplEmitValueIr> = (0..count)
            .map(|i| SrplEmitValueIr {
                column: format!("col_{}", i),
                value: arith(ArithOp::Add, int(i as i64), int(1)),
            })
            .collect();
        let result = fold_emit_values(values.clone());
        prop_assert_eq!(
            result.len(), count,
            "fold_emit_values must preserve emit value count"
        );
    });
}

/// Property: fold_assignments preserves assignment field names.
#[test]
fn t_pr_11_fold_assignments_preserves_field_names() {
    proptest!(|(count in 1usize..8)| {
        let assignments: Vec<SrplAssignmentIr> = (0..count)
            .map(|i| simple_assignment(&format!("field_{}", i), int(i as i64)))
            .collect();
        let field_names: Vec<String> = assignments.iter().map(|a| a.field.clone()).collect();
        let result = fold_assignments(assignments);
        let result_names: Vec<String> = result.iter().map(|a| a.field.clone()).collect();
        prop_assert_eq!(field_names, result_names, "field names must be preserved by fold_assignments");
    });
}
