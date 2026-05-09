use andromeda_srpl_ir::{ArithOp, ConstantLiteral, SrplValueIr};

/// Reason a constant fold was deferred.
///
/// Used internally only; never surfaces as a user-visible error.
/// A deferred fold leaves the `BinaryArith` node unchanged in the IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldDeferral {
    /// Divisor is the integer 0 — runtime error if actually reached.
    DivisionByZero,
    /// Arithmetic result overflows the target integer type.
    Overflow,
    /// The two literal types are not directly compatible for this
    /// operator (e.g. `Int64 + Uint64`).
    TypeMismatch,
}

/// Fold a single `SrplValueIr` node.
///
/// Returns `Ok(folded)` on success (including a successful no-op when
/// the node is not foldable). A `FoldDeferral` error indicates the
/// fold was deferred; the caller must leave the original node in the IR.
///
/// # Deferral contract (INV-09)
/// Division-by-zero and arithmetic overflow are **never** raised as
/// errors to the caller. The `BinaryArith` node is reconstructed with
/// best-effort-folded children and returned as `Ok(BinaryArith { .. })`.
pub fn fold_value(value: SrplValueIr) -> Result<SrplValueIr, FoldDeferral> {
    match value {
        SrplValueIr::BinaryArith { op, left, right } => {
            // Recursively fold sub-trees first, best-effort.
            let left_folded = fold_value_best_effort(*left);
            let right_folded = fold_value_best_effort(*right);

            // Attempt constant combination only when both sides folded
            // into constants.
            match (&left_folded, &right_folded) {
                (SrplValueIr::Constant(l), SrplValueIr::Constant(r)) => {
                    match fold_binary_constants(op, l, r) {
                        Ok(result) => Ok(SrplValueIr::Constant(result)),
                        // INV-09: defer division-by-zero / overflow.
                        // Reassemble the node; no error propagated.
                        Err(_deferred) => Ok(SrplValueIr::BinaryArith {
                            op,
                            left: Box::new(left_folded),
                            right: Box::new(right_folded),
                        }),
                    }
                },
                // One or both operands are not constant: reassemble with
                // the best-effort-folded children.
                _ => Ok(SrplValueIr::BinaryArith {
                    op,
                    left: Box::new(left_folded),
                    right: Box::new(right_folded),
                }),
            }
        },

        // All other variants are already in their canonical IR form.
        other => Ok(other),
    }
}

/// Fold `value`, returning the original if fold yields an error.
///
/// This is used internally to fold sub-trees where a fold failure
/// should be silently ignored (the parent node handles deferral).
fn fold_value_best_effort(value: SrplValueIr) -> SrplValueIr {
    // `fold_value` only returns `Err(FoldDeferral)` for the top-level
    // operation; all sub-tree errors are absorbed internally and returned
    // as `Ok(BinaryArith { .. })`. This function is therefore always safe
    // to call — the `unwrap_or` guard is a defensive belt-and-suspenders.
    fold_value(value.clone()).unwrap_or(value)
}

/// Attempt to combine two `ConstantLiteral` values under `op`.
///
/// Returns `Err(FoldDeferral)` when the operation cannot be performed
/// at compile time (division by zero, overflow, type mismatch).
fn fold_binary_constants(
    op: ArithOp,
    left: &ConstantLiteral,
    right: &ConstantLiteral,
) -> Result<ConstantLiteral, FoldDeferral> {
    match (op, left, right) {
        (ArithOp::Add, ConstantLiteral::Int64(a), ConstantLiteral::Int64(b)) => a
            .checked_add(*b)
            .map(ConstantLiteral::Int64)
            .ok_or(FoldDeferral::Overflow),

        (ArithOp::Subtract, ConstantLiteral::Int64(a), ConstantLiteral::Int64(b)) => a
            .checked_sub(*b)
            .map(ConstantLiteral::Int64)
            .ok_or(FoldDeferral::Overflow),

        (ArithOp::Multiply, ConstantLiteral::Int64(a), ConstantLiteral::Int64(b)) => a
            .checked_mul(*b)
            .map(ConstantLiteral::Int64)
            .ok_or(FoldDeferral::Overflow),

        (ArithOp::Divide, ConstantLiteral::Int64(_), ConstantLiteral::Int64(0)) => {
            Err(FoldDeferral::DivisionByZero)
        },

        (ArithOp::Divide, ConstantLiteral::Int64(a), ConstantLiteral::Int64(b)) => a
            .checked_div(*b)
            .map(ConstantLiteral::Int64)
            .ok_or(FoldDeferral::Overflow),

        (ArithOp::Add, ConstantLiteral::Uint64(a), ConstantLiteral::Uint64(b)) => a
            .checked_add(*b)
            .map(ConstantLiteral::Uint64)
            .ok_or(FoldDeferral::Overflow),

        (ArithOp::Subtract, ConstantLiteral::Uint64(a), ConstantLiteral::Uint64(b)) => a
            .checked_sub(*b)
            .map(ConstantLiteral::Uint64)
            .ok_or(FoldDeferral::Overflow),

        (ArithOp::Multiply, ConstantLiteral::Uint64(a), ConstantLiteral::Uint64(b)) => a
            .checked_mul(*b)
            .map(ConstantLiteral::Uint64)
            .ok_or(FoldDeferral::Overflow),

        (ArithOp::Divide, ConstantLiteral::Uint64(_), ConstantLiteral::Uint64(0)) => {
            Err(FoldDeferral::DivisionByZero)
        },

        (ArithOp::Divide, ConstantLiteral::Uint64(a), ConstantLiteral::Uint64(b)) => a
            .checked_div(*b)
            .map(ConstantLiteral::Uint64)
            .ok_or(FoldDeferral::Overflow),

        _ => Err(FoldDeferral::TypeMismatch),
    }
}

/// Apply constant folding to all value nodes in an assignment list.
pub fn fold_assignments(
    assignments: Vec<andromeda_srpl_ir::SrplAssignmentIr>,
) -> Vec<andromeda_srpl_ir::SrplAssignmentIr> {
    assignments
        .into_iter()
        .map(|mut a| {
            a.value = fold_value(a.value.clone()).unwrap_or(a.value);
            a
        })
        .collect()
}

/// Apply constant folding to all value nodes in an emit value list.
pub fn fold_emit_values(
    values: Vec<andromeda_srpl_ir::SrplEmitValueIr>,
) -> Vec<andromeda_srpl_ir::SrplEmitValueIr> {
    values
        .into_iter()
        .map(|mut ev| {
            ev.value = fold_value(ev.value.clone()).unwrap_or(ev.value);
            ev
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_srpl_ir::{ArithOp, ConstantLiteral, SrplValueIr};

    fn int(n: i64) -> SrplValueIr {
        SrplValueIr::Constant(ConstantLiteral::Int64(n))
    }

    fn uint(n: u64) -> SrplValueIr {
        SrplValueIr::Constant(ConstantLiteral::Uint64(n))
    }

    fn arith(op: ArithOp, l: SrplValueIr, r: SrplValueIr) -> SrplValueIr {
        SrplValueIr::BinaryArith {
            op,
            left: Box::new(l),
            right: Box::new(r),
        }
    }

    // T-CF-01
    #[test]
    fn fold_add_int64() {
        let result = fold_value(arith(ArithOp::Add, int(3), int(4))).unwrap();
        assert_eq!(result, int(7));
    }

    // T-CF-02
    #[test]
    fn fold_subtract_int64() {
        let result = fold_value(arith(ArithOp::Subtract, int(10), int(4))).unwrap();
        assert_eq!(result, int(6));
    }

    // T-CF-03
    #[test]
    fn fold_multiply_int64() {
        let result = fold_value(arith(ArithOp::Multiply, int(6), int(7))).unwrap();
        assert_eq!(result, int(42));
    }

    // T-CF-04
    #[test]
    fn fold_divide_int64() {
        let result = fold_value(arith(ArithOp::Divide, int(10), int(2))).unwrap();
        assert_eq!(result, int(5));
    }

    // T-CF-05 — INV-09: division by zero deferred to runtime
    #[test]
    fn fold_divide_by_zero_is_deferred() {
        let original = arith(ArithOp::Divide, int(10), int(0));
        let result = fold_value(original.clone()).unwrap();
        // Must remain a BinaryArith node, not raise an error
        assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
    }

    // T-CF-06 — overflow deferred
    #[test]
    fn fold_overflow_is_deferred() {
        let result = fold_value(arith(ArithOp::Add, int(i64::MAX), int(1))).unwrap();
        assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
    }

    // T-CF-07 — Bool normalisation
    #[test]
    fn fold_bool_normalises_to_constant() {
        let result = fold_value(SrplValueIr::bool(true)).unwrap();
        assert_eq!(result, SrplValueIr::Constant(ConstantLiteral::Bool(true)));
    }

    // T-CF-08 — non-constant operand stays as BinaryArith
    #[test]
    fn fold_non_constant_operand_stays() {
        let val = arith(ArithOp::Add, SrplValueIr::Input("x".into()), int(0));
        let result = fold_value(val).unwrap();
        assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
    }

    // T-CF-09 — nested folding
    #[test]
    fn fold_nested_expression() {
        // (2 * 3) + 4 = 10
        let inner = arith(ArithOp::Multiply, int(2), int(3));
        let outer = arith(ArithOp::Add, inner, int(4));
        let result = fold_value(outer).unwrap();
        assert_eq!(result, int(10));
    }

    // T-CF-10 — type mismatch deferred
    #[test]
    fn fold_mixed_types_deferred() {
        let result = fold_value(arith(ArithOp::Add, uint(5), int(3))).unwrap();
        assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
    }

    #[test]
    fn fold_is_idempotent_on_constant_add() {
        let expr = arith(ArithOp::Add, int(3), int(4));
        let once = fold_value(expr).unwrap();
        let twice = fold_value(once.clone()).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn fold_uint64_arithmetic() {
        assert_eq!(
            fold_value(arith(ArithOp::Add, uint(10), uint(5))).unwrap(),
            uint(15)
        );
        assert_eq!(
            fold_value(arith(ArithOp::Subtract, uint(10), uint(3))).unwrap(),
            uint(7)
        );
        assert_eq!(
            fold_value(arith(ArithOp::Multiply, uint(4), uint(3))).unwrap(),
            uint(12)
        );
        assert_eq!(
            fold_value(arith(ArithOp::Divide, uint(12), uint(4))).unwrap(),
            uint(3)
        );
    }

    #[test]
    fn fold_uint64_divide_by_zero_deferred() {
        let result = fold_value(arith(ArithOp::Divide, uint(10), uint(0))).unwrap();
        assert!(matches!(result, SrplValueIr::BinaryArith { .. }));
    }
}
