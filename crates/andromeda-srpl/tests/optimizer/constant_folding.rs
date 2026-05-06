/// T-CF-01  1 + 1 → 2
#[test]
fn t_cf_01_fold_add_one_plus_one() {
    let result = fold_value(arith(ArithOp::Add, int(1), int(1))).unwrap();
    assert_eq!(result, int(2));
}

/// T-CF-02  10 - 5 → 5
#[test]
fn t_cf_02_fold_subtract_ten_minus_five() {
    let result = fold_value(arith(ArithOp::Subtract, int(10), int(5))).unwrap();
    assert_eq!(result, int(5));
}

/// T-CF-03  2 * 3 → 6
#[test]
fn t_cf_03_fold_multiply_two_times_three() {
    let result = fold_value(arith(ArithOp::Multiply, int(2), int(3))).unwrap();
    assert_eq!(result, int(6));
}

/// T-CF-04  10 / 2 → 5
#[test]
fn t_cf_04_fold_divide_ten_by_two() {
    let result = fold_value(arith(ArithOp::Divide, int(10), int(2))).unwrap();
    assert_eq!(result, int(5));
}

/// T-CF-05  -5 + 3 → -2
#[test]
fn t_cf_05_fold_negative_add() {
    let result = fold_value(arith(ArithOp::Add, int(-5), int(3))).unwrap();
    assert_eq!(result, int(-2));
}

/// T-CF-06  0 - 7 → -7
#[test]
fn t_cf_06_fold_zero_minus_seven() {
    let result = fold_value(arith(ArithOp::Subtract, int(0), int(7))).unwrap();
    assert_eq!(result, int(-7));
}

/// T-CF-07  0 * 999 → 0
#[test]
fn t_cf_07_fold_multiply_by_zero() {
    let result = fold_value(arith(ArithOp::Multiply, int(0), int(999))).unwrap();
    assert_eq!(result, int(0));
}

/// T-CF-08  1 / 1 → 1
#[test]
fn t_cf_08_fold_divide_one_by_one() {
    let result = fold_value(arith(ArithOp::Divide, int(1), int(1))).unwrap();
    assert_eq!(result, int(1));
}

/// T-CF-09  u64(10) + u64(5) → u64(15)
#[test]
fn t_cf_09_fold_uint64_add() {
    let result = fold_value(arith(ArithOp::Add, uint(10), uint(5))).unwrap();
    assert_eq!(result, uint(15));
}

/// T-CF-10  u64(100) - u64(37) → u64(63)
#[test]
fn t_cf_10_fold_uint64_subtract() {
    let result = fold_value(arith(ArithOp::Subtract, uint(100), uint(37))).unwrap();
    assert_eq!(result, uint(63));
}

/// T-CF-11  u64(7) * u64(8) → u64(56)
#[test]
fn t_cf_11_fold_uint64_multiply() {
    let result = fold_value(arith(ArithOp::Multiply, uint(7), uint(8))).unwrap();
    assert_eq!(result, uint(56));
}

/// T-CF-12  u64(100) / u64(4) → u64(25)
#[test]
fn t_cf_12_fold_uint64_divide() {
    let result = fold_value(arith(ArithOp::Divide, uint(100), uint(4))).unwrap();
    assert_eq!(result, uint(25));
}

/// T-CF-13  INT64_MAX + 1 → deferred (Overflow), BinaryArith preserved.
#[test]
fn t_cf_13_int64_max_plus_one_deferred() {
    let expr = arith(ArithOp::Add, int(i64::MAX), int(1));
    let result = fold_value(expr).unwrap();
    assert!(
        matches!(result, SrplValueIr::BinaryArith { .. }),
        "INT64_MAX + 1 must be deferred to runtime, not produce a constant"
    );
}

/// T-CF-14  INT64_MIN - 1 → deferred (Overflow), BinaryArith preserved.
#[test]
fn t_cf_14_int64_min_minus_one_deferred() {
    let expr = arith(ArithOp::Subtract, int(i64::MIN), int(1));
    let result = fold_value(expr).unwrap();
    assert!(
        matches!(result, SrplValueIr::BinaryArith { .. }),
        "INT64_MIN - 1 must be deferred to runtime"
    );
}

/// T-CF-15  INT64_MAX * 2 → deferred (Overflow).
#[test]
fn t_cf_15_int64_max_multiply_deferred() {
    let expr = arith(ArithOp::Multiply, int(i64::MAX), int(2));
    let result = fold_value(expr).unwrap();
    assert!(
        matches!(result, SrplValueIr::BinaryArith { .. }),
        "INT64_MAX * 2 must be deferred"
    );
}

/// T-CF-16  i64(10) / i64(0) → deferred (DivisionByZero), BinaryArith preserved.
#[test]
fn t_cf_16_int64_divide_by_zero_deferred() {
    let expr = arith(ArithOp::Divide, int(10), int(0));
    let result = fold_value(expr).unwrap();
    assert!(
        matches!(result, SrplValueIr::BinaryArith { .. }),
        "Division by zero must leave a BinaryArith node (INV-09)"
    );
}

/// T-CF-17  u64(5) / u64(0) → deferred, not panicked.
#[test]
fn t_cf_17_uint64_divide_by_zero_deferred() {
    let expr = arith(ArithOp::Divide, uint(5), uint(0));
    let result = fold_value(expr).unwrap();
    assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
}

/// T-CF-18  u64(UINT64_MAX) + u64(1) → deferred (Overflow).
#[test]
fn t_cf_18_uint64_max_plus_one_deferred() {
    let expr = arith(ArithOp::Add, uint(u64::MAX), uint(1));
    let result = fold_value(expr).unwrap();
    assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
}

/// T-CF-19  u64(0) - u64(1) → deferred (underflow wraps on unsigned → overflow branch).
#[test]
fn t_cf_19_uint64_underflow_deferred() {
    let expr = arith(ArithOp::Subtract, uint(0), uint(1));
    let result = fold_value(expr).unwrap();
    assert!(
        matches!(result, SrplValueIr::BinaryArith { .. }),
        "u64 subtraction underflow must be deferred"
    );
}

/// T-CF-20  Bool(true) → Constant(Bool(true))  (deprecated variant migration).
#[test]
fn t_cf_20_bool_true_normalises_to_constant() {
    let result = fold_value(SrplValueIr::Bool(true)).unwrap();
    assert_eq!(result, SrplValueIr::Constant(ConstantLiteral::Bool(true)));
}

/// T-CF-21  Bool(false) → Constant(Bool(false)).
#[test]
fn t_cf_21_bool_false_normalises_to_constant() {
    let result = fold_value(SrplValueIr::Bool(false)).unwrap();
    assert_eq!(result, SrplValueIr::Constant(ConstantLiteral::Bool(false)));
}

/// T-CF-22  Constant(Bool(true)) remains unchanged (already canonical).
#[test]
fn t_cf_22_constant_bool_already_canonical() {
    let v = bool_const(true);
    let result = fold_value(v.clone()).unwrap();
    assert_eq!(
        result, v,
        "already-canonical Constant(Bool) must not be mutated"
    );
}

/// T-CF-23  Int64 + Uint64 → deferred (TypeMismatch).
#[test]
fn t_cf_23_mixed_int_uint_add_deferred() {
    let expr = arith(ArithOp::Add, int(5), uint(3));
    let result = fold_value(expr).unwrap();
    assert!(
        matches!(result, SrplValueIr::BinaryArith { .. }),
        "Int64 + Uint64 is a TypeMismatch: must be deferred"
    );
}

/// T-CF-24  Uint64 - Int64 → deferred.
#[test]
fn t_cf_24_uint_minus_int_deferred() {
    let expr = arith(ArithOp::Subtract, uint(10), int(3));
    let result = fold_value(expr).unwrap();
    assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
}

/// T-CF-25  Bool + Int64 → deferred (no such mixed operation).
#[test]
fn t_cf_25_bool_add_int_deferred() {
    let expr = arith(ArithOp::Add, bool_const(true), int(1));
    let result = fold_value(expr).unwrap();
    assert!(
        matches!(result, SrplValueIr::BinaryArith { .. }),
        "Bool + Int64 must be deferred as TypeMismatch"
    );
}

/// T-CF-26  Input("x") + Int(0) → BinaryArith (non-constant left).
#[test]
fn t_cf_26_input_plus_constant_stays_binary_arith() {
    let expr = arith(ArithOp::Add, SrplValueIr::Input("x".into()), int(0));
    let result = fold_value(expr).unwrap();
    assert!(
        matches!(result, SrplValueIr::BinaryArith { .. }),
        "Non-constant operand prevents folding"
    );
}

/// T-CF-27  Int(0) + Input("x") → BinaryArith (non-constant right).
#[test]
fn t_cf_27_constant_plus_input_stays_binary_arith() {
    let expr = arith(ArithOp::Add, int(0), SrplValueIr::Input("x".into()));
    let result = fold_value(expr).unwrap();
    assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
}

/// T-CF-28  Field{T,id} * Int(2) → BinaryArith (runtime field).
#[test]
fn t_cf_28_field_ref_prevents_fold() {
    let field = SrplValueIr::Field {
        binding: "T".into(),
        field: "id".into(),
    };
    let expr = arith(ArithOp::Multiply, field, int(2));
    let result = fold_value(expr).unwrap();
    assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
}

/// T-CF-29  SubtractInput{T,qty,amount} stays as BinaryArith (runtime).
#[test]
fn t_cf_29_subtract_input_value_not_foldable() {
    let sub = SrplValueIr::SubtractInput {
        binding: "T".into(),
        field: "qty".into(),
        input: "amount".into(),
    };
    let expr = arith(ArithOp::Add, sub, int(0));
    let result = fold_value(expr).unwrap();
    assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
}

/// T-CF-30  Input stays as Input when passed directly (already canonical).
#[test]
fn t_cf_30_input_passthrough() {
    let v = SrplValueIr::Input("param".into());
    let result = fold_value(v.clone()).unwrap();
    assert_eq!(result, v);
}

/// T-CF-31  (2 * 3) + 4 → 10
#[test]
fn t_cf_31_nested_fold_mul_then_add() {
    let inner = arith(ArithOp::Multiply, int(2), int(3));
    let outer = arith(ArithOp::Add, inner, int(4));
    assert_eq!(fold_value(outer).unwrap(), int(10));
}

/// T-CF-32  (10 - 4) * (2 + 1) → 18
#[test]
fn t_cf_32_nested_fold_both_sides() {
    let left = arith(ArithOp::Subtract, int(10), int(4));
    let right = arith(ArithOp::Add, int(2), int(1));
    let expr = arith(ArithOp::Multiply, left, right);
    assert_eq!(fold_value(expr).unwrap(), int(18));
}

/// T-CF-33  ((1 + 1) + (1 + 1)) + ((1 + 1) + (1 + 1)) → 8
#[test]
fn t_cf_33_deeply_nested_folds_to_single_constant() {
    let pair = || arith(ArithOp::Add, int(1), int(1)); // 2
    let quad = || arith(ArithOp::Add, pair(), pair()); // 4
    let expr = arith(ArithOp::Add, quad(), quad()); // 8
    assert_eq!(fold_value(expr).unwrap(), int(8));
}

/// T-CF-34  Partial fold: (2 + 3) + Input("x") → BinaryArith with Constant(5) left child.
#[test]
fn t_cf_34_partial_fold_constant_subtree_folded() {
    let left = arith(ArithOp::Add, int(2), int(3));
    let expr = arith(ArithOp::Add, left, SrplValueIr::Input("x".into()));
    let result = fold_value(expr).unwrap();
    // The result must be a BinaryArith because the right side is not constant.
    assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
    // But the left child must have been folded to Constant(5).
    if let SrplValueIr::BinaryArith { left, .. } = result {
        assert_eq!(*left, int(5), "left sub-tree (2+3) must be folded to 5");
    }
}

/// T-CF-35  MAX_EXPR_DEPTH node at maximum depth is accepted (depth = MAX_EXPR_DEPTH).
#[test]
fn t_cf_35_expression_at_max_depth_is_valid() {
    // Build a left-leaning tree of depth MAX_EXPR_DEPTH.
    let mut expr = int(1);
    for _ in 0..MAX_EXPR_DEPTH {
        expr = arith(ArithOp::Add, expr, int(0));
    }
    // depth() == MAX_EXPR_DEPTH at this point; fold should produce a constant.
    let result = fold_value(expr).unwrap();
    assert_eq!(result, int(1));
}

/// T-CF-36  fold_assignments normalises Bool in an assignment value.
#[test]
fn t_cf_36_fold_assignments_normalises_bool() {
    let assignments = vec![simple_assignment("flag", SrplValueIr::Bool(false))];
    let result = fold_assignments(assignments);
    assert_eq!(
        result[0].value,
        SrplValueIr::Constant(ConstantLiteral::Bool(false))
    );
}

/// T-CF-37  fold_assignments folds a BinaryArith in an assignment value.
#[test]
fn t_cf_37_fold_assignments_folds_arith() {
    let assignments = vec![simple_assignment(
        "qty",
        arith(ArithOp::Add, int(3), int(4)),
    )];
    let result = fold_assignments(assignments);
    assert_eq!(result[0].value, int(7));
}

/// T-CF-38  fold_assignments with multiple assignments folds each independently.
#[test]
fn t_cf_38_fold_multiple_assignments() {
    let assignments = vec![
        simple_assignment("a", arith(ArithOp::Add, int(1), int(2))),
        simple_assignment("b", arith(ArithOp::Multiply, int(3), int(3))),
        simple_assignment("c", SrplValueIr::Input("p".into())),
    ];
    let result = fold_assignments(assignments);
    assert_eq!(result[0].value, int(3));
    assert_eq!(result[1].value, int(9));
    assert_eq!(result[2].value, SrplValueIr::Input("p".into()));
}

/// T-CF-39  fold_emit_values normalises Bool in an emit value.
#[test]
fn t_cf_39_fold_emit_values_normalises_bool() {
    let values = vec![SrplEmitValueIr {
        column: "active".into(),
        value: SrplValueIr::Bool(true),
    }];
    let result = fold_emit_values(values);
    assert_eq!(
        result[0].value,
        SrplValueIr::Constant(ConstantLiteral::Bool(true))
    );
}

/// T-CF-40  fold_emit_values folds arithmetic.
#[test]
fn t_cf_40_fold_emit_values_folds_arith() {
    let values = vec![SrplEmitValueIr {
        column: "sum".into(),
        value: arith(ArithOp::Add, uint(20), uint(22)),
    }];
    let result = fold_emit_values(values);
    assert_eq!(result[0].value, uint(42));
}

/// T-CF-41  fold_value is idempotent: fold(fold(v)) == fold(v).
#[test]
fn t_cf_41_fold_is_idempotent() {
    let cases: Vec<SrplValueIr> = vec![
        arith(ArithOp::Add, int(3), int(4)),
        arith(ArithOp::Divide, int(10), int(0)),
        SrplValueIr::Bool(true),
        arith(ArithOp::Add, uint(u64::MAX), uint(1)),
    ];
    for expr in cases {
        let once = fold_value(expr).unwrap();
        let twice = fold_value(once.clone()).unwrap();
        assert_eq!(once, twice, "fold must be idempotent");
    }
}

/// T-CF-42  Constant(Int64) passthrough: already canonical, unchanged.
#[test]
fn t_cf_42_constant_int64_passthrough() {
    let v = int(42);
    let result = fold_value(v.clone()).unwrap();
    assert_eq!(result, v);
}

/// T-CF-43  Constant(Uint64) passthrough.
#[test]
fn t_cf_43_constant_uint64_passthrough() {
    let v = uint(100);
    let result = fold_value(v.clone()).unwrap();
    assert_eq!(result, v);
}

/// T-CF-44  fold_value always returns Ok (never Err for the public API).
#[test]
fn t_cf_44_fold_value_public_api_always_ok() {
    // Even when there is a deferral internally the public Result is Ok.
    let dangerous_cases = vec![
        arith(ArithOp::Divide, int(1), int(0)),
        arith(ArithOp::Add, int(i64::MAX), int(1)),
        arith(ArithOp::Add, int(5), uint(5)), // type mismatch
    ];
    for expr in dangerous_cases {
        let r = fold_value(expr);
        assert!(
            r.is_ok(),
            "fold_value must always return Ok (deferral is internal)"
        );
    }
}

/// T-CF-45  Large integer constants fold without overflow: i64::MAX / i64::MAX = 1.
#[test]
fn t_cf_45_large_constant_divide_self() {
    let expr = arith(ArithOp::Divide, int(i64::MAX), int(i64::MAX));
    let result = fold_value(expr).unwrap();
    assert_eq!(result, int(1));
}
