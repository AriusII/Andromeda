use super::*;

/// T-DT-01  Same IR input always produces the same fold result (determinism).
#[test]
fn t_dt_01_fold_deterministic_on_identical_input() {
    let expr = arith(ArithOp::Add, int(42), int(58));
    let r1 = fold_value(expr.clone()).unwrap();
    let r2 = fold_value(expr).unwrap();
    assert_eq!(r1, r2, "fold_value must be deterministic");
}

/// T-DT-02  Same IR always normalizes to the same canonical form.
#[test]
fn t_dt_02_normalize_deterministic() {
    let p = eq_pred("b", "T", "b");
    let q = eq_pred("a", "T", "a");
    let ir1 = make_ir(vec![read_op(0, "T", vec![p.clone(), q.clone()])]);
    let ir2 = make_ir(vec![read_op(0, "T", vec![p, q])]);
    let n1 = normalize(ir1).unwrap();
    let n2 = normalize(ir2).unwrap();
    assert_eq!(
        n1, n2,
        "normalize must produce identical output for identical input"
    );
}

/// T-DT-03  Same IR always produces the same cost estimate (determinism).
#[test]
fn t_dt_03_cost_estimate_deterministic() {
    let ir = make_ir(vec![read_op(0, "T", vec![eq_pred("id", "T", "id")])]);
    let c1 = estimate_without_stats(&ir);
    let c2 = estimate_without_stats(&ir);
    assert_eq!(c1.total_cost, c2.total_cost);
}

/// T-DT-04  Same alternatives always produce the same plan choice.
#[test]
fn t_dt_04_plan_choice_deterministic() {
    let mk_alts = || {
        vec![
            (empty_ir(), cost(100.0), OptimizerPlanKind::RangeScan),
            (empty_ir(), cost(20.0), OptimizerPlanKind::PointLookup),
        ]
    };
    let r1 = choose(mk_alts()).unwrap();
    let r2 = choose(mk_alts()).unwrap();
    assert_eq!(r1.chosen_kind, r2.chosen_kind);
    assert_eq!(r1.chosen_cost.total_cost, r2.chosen_cost.total_cost);
}

/// T-DT-05  Same predicate list always produces the same simplified result.
#[test]
fn t_dt_05_predicate_simplification_deterministic() {
    let preds = vec![
        eq_pred("a", "T", "a"),
        eq_pred("a", "T", "a"),
        eq_pred("b", "T", "b"),
    ];
    let r1 = simplify_predicates(preds.clone());
    let r2 = simplify_predicates(preds);
    assert_eq!(r1, r2);
}

/// T-DT-06  Repeated application of proj_apply on the same IR always produces
///          the same projection set (annotation-only, no mutation).
#[test]
fn t_dt_06_proj_apply_deterministic() {
    let ir = make_ir(vec![read_op(0, "T", vec![]), emit_op(1, "T", "id")]);
    let r1 = proj_apply(ir.clone());
    let r2 = proj_apply(ir);
    assert_eq!(r1.projections.len(), r2.projections.len());
    for (p1, p2) in r1.projections.iter().zip(r2.projections.iter()) {
        assert_eq!(p1.binding, p2.binding);
        assert_eq!(p1.live_columns, p2.live_columns);
    }
}

/// T-CR-01  fold_value on a deeply-nested overflow chain does not panic.
#[test]
fn t_cr_01_overflow_chain_does_not_panic() {
    let mut expr = int(i64::MAX);
    for _ in 0..MAX_EXPR_DEPTH {
        expr = arith(ArithOp::Add, expr, int(1));
    }
    // Must not panic; may defer.
    let result = fold_value(expr);
    assert!(result.is_ok(), "deeply nested overflow must not panic");
}

/// T-CR-02  normalize on a body with maximum allowed operations succeeds.
#[test]
fn t_cr_02_normalize_at_max_body_ops_succeeds() {
    let ops: Vec<SrplBusinessOperationIr> =
        (0..MAX_SRPL_BODY_OPERATIONS as u32).map(raise_op).collect();
    let ir = make_ir(ops);
    assert!(
        normalize(ir).is_ok(),
        "body at exactly MAX_SRPL_BODY_OPERATIONS must normalize"
    );
}

/// T-CR-03  validate_bounded rejects a body exceeding MAX_SRPL_BODY_OPERATIONS.
#[test]
fn t_cr_03_validate_bounded_rejects_over_limit() {
    let ops: Vec<SrplBusinessOperationIr> = (0..(MAX_SRPL_BODY_OPERATIONS + 1) as u32)
        .map(raise_op)
        .collect();
    let body = SrplProcedureBodyIr { operations: ops };
    assert!(
        body.validate_bounded().is_err(),
        "body exceeding MAX_SRPL_BODY_OPERATIONS must fail validation"
    );
}

/// T-CR-04  validate_bounded rejects non-dense ordinals.
#[test]
fn t_cr_04_validate_bounded_rejects_non_dense_ordinals() {
    let body = SrplProcedureBodyIr {
        operations: vec![
            SrplBusinessOperationIr {
                ordinal: 0,
                kind: raise_op(0).kind,
            },
            SrplBusinessOperationIr {
                ordinal: 5,
                kind: raise_op(5).kind,
            }, // gap
        ],
    };
    assert!(
        body.validate_bounded().is_err(),
        "non-dense ordinals must fail validation"
    );
}

/// T-CR-05  pushdown_apply on a body at MAX_SRPL_BODY_OPERATIONS does not corrupt ordinals.
#[test]
fn t_cr_05_pushdown_at_op_limit_safe() {
    // Fill with Raise operations up to the limit (no asserts to push).
    let ops: Vec<SrplBusinessOperationIr> =
        (0..MAX_SRPL_BODY_OPERATIONS as u32).map(raise_op).collect();
    let ir = make_ir(ops);
    let result = pushdown_apply(ir).unwrap();
    for (i, op) in result.body.operations.iter().enumerate() {
        assert_eq!(
            op.ordinal, i as u32,
            "ordinals must be dense after pushdown at limit"
        );
    }
}

/// T-CR-06  choose returns a descriptive error when alternatives is empty (no panic).
#[test]
fn t_cr_06_choose_empty_alternatives_error_not_panic() {
    let result = choose(vec![]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(
        !msg.is_empty(),
        "error from empty alternatives must have a non-empty description"
    );
}

/// T-CR-07  Constant with invalid decimal scale returns validation error.
#[test]
fn t_cr_07_decimal_scale_exceeds_18_rejects() {
    let lit = ConstantLiteral::Decimal {
        integer_part: 123,
        scale: 19,
    };
    assert!(
        lit.validate().is_err(),
        "Decimal with scale > 18 must fail validation"
    );
}

/// T-CR-08  Constant with valid decimal scale validates without error.
#[test]
fn t_cr_08_decimal_scale_at_18_valid() {
    let lit = ConstantLiteral::Decimal {
        integer_part: 1,
        scale: 18,
    };
    assert!(
        lit.validate().is_ok(),
        "Decimal with scale = 18 must be valid"
    );
}

/// T-CR-09  type_tag returns distinct bytes for each ConstantLiteral variant.
#[test]
fn t_cr_09_constant_literal_type_tags_are_distinct() {
    let tags = [
        ConstantLiteral::Bool(true).type_tag(),
        ConstantLiteral::Int64(0).type_tag(),
        ConstantLiteral::Uint64(0).type_tag(),
        ConstantLiteral::Decimal {
            integer_part: 0,
            scale: 0,
        }
        .type_tag(),
    ];
    let unique: std::collections::BTreeSet<u8> = tags.iter().cloned().collect();
    assert_eq!(
        unique.len(),
        tags.len(),
        "every ConstantLiteral variant must have a distinct type_tag"
    );
}

/// T-CR-10  ArithOp::as_tag returns distinct bytes for each operator.
#[test]
fn t_cr_10_arith_op_tags_are_distinct() {
    let tags = [
        ArithOp::Add.as_tag(),
        ArithOp::Subtract.as_tag(),
        ArithOp::Multiply.as_tag(),
        ArithOp::Divide.as_tag(),
    ];
    let unique: std::collections::BTreeSet<u8> = tags.iter().cloned().collect();
    assert_eq!(
        unique.len(),
        4,
        "all ArithOp variants must have distinct tags"
    );
}

/// T-CR-11  SrplValueIr::is_constant returns true for constant trees, false for runtime refs.
#[test]
fn t_cr_11_is_constant_reflects_tree_structure() {
    assert!(
        int(5).is_constant(),
        "Int64 constant must report is_constant = true"
    );
    assert!(
        uint(10).is_constant(),
        "Uint64 constant must report is_constant = true"
    );
    assert!(
        SrplValueIr::bool(false).is_constant(),
        "Bool must report is_constant = true"
    );
    assert!(
        bool_const(true).is_constant(),
        "Constant(Bool) must report is_constant = true"
    );
    assert!(
        !SrplValueIr::Input("x".into()).is_constant(),
        "Input must report is_constant = false"
    );
    assert!(
        arith(ArithOp::Add, int(1), int(2)).is_constant(),
        "BinaryArith over constants must report is_constant = true"
    );
    assert!(
        !arith(ArithOp::Add, SrplValueIr::Input("x".into()), int(2)).is_constant(),
        "BinaryArith with Input child must report is_constant = false"
    );
}

/// T-CR-12  ConstantLiteral::is_compatible_with type checks are exact (no silent widening).
#[test]
fn t_cr_12_constant_literal_type_compatibility() {
    assert!(ConstantLiteral::Bool(true).is_compatible_with(&ScalarType::Bool));
    assert!(!ConstantLiteral::Bool(true).is_compatible_with(&ScalarType::I64));
    assert!(ConstantLiteral::Int64(0).is_compatible_with(&ScalarType::I64));
    assert!(!ConstantLiteral::Int64(0).is_compatible_with(&ScalarType::U64));
    assert!(ConstantLiteral::Uint64(0).is_compatible_with(&ScalarType::U64));
    assert!(!ConstantLiteral::Uint64(0).is_compatible_with(&ScalarType::I64));
}
