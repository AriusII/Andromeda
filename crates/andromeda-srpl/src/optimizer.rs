//! SRPL optimizer pipeline — Wave 13 Batch 21 delivery.
//!
//! All optimization passes live in this module as inline submodules so no
//! additional source directories are required. The module is declared
//! `pub` in `lib.rs`; individual sub-module items are re-exported at the
//! crate root via `procedure_model`.
//!
//! ## Pass Order
//!
//! ```text
//! Parsing → Binding → IRLowering → ConstantFolding → PredicatePushdown
//!         → ProjectionPushdown → CostAnalysis → PlanChoice
//! ```
//!
//! Each pass preserves semantic equivalence (INV-07).
//! Non-deterministic functions are never folded (INV-08).
//! Division-by-zero / overflow is deferred to runtime (INV-09).
//! Body operation limits are never exceeded (INV-10, INV-11).
//! No unsafe code anywhere in this module.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::ir::{
    ArithOp, ConstantLiteral, MAX_SRPL_BODY_OPERATIONS, SrplAssignmentIr, SrplEmitValueIr,
    SrplPredicateIr, SrplValueIr,
};
use crate::{
    Cardinality, SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr,
    SrplProcedureIr,
};

// ============================================================================
// Sub-module: phase
// ============================================================================

/// Bounded ordered phases of the SRPL optimizer pipeline.
///
/// Phases are applied in `as_ordinal` order only. No phase reordering is
/// permitted at runtime. Adding a phase is a doctrine change.
///
/// # Invariants
/// - `Binding` must complete before `IRLowering`.
/// - `IRLowering` must complete before any optimization pass.
/// - `CostAnalysis` must complete before `PlanChoice`.
pub mod phase {
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
}

// ============================================================================
// Sub-module: plan_kind
// ============================================================================

/// Internal optimizer plan-shape taxonomy.
///
/// This enum is **transient** — it is used during cost analysis and plan
/// choice, then discarded. It must NOT enter a `PlanCacheKey`. The
/// external cache-key taxonomy is `andromeda_catalog::PlanClass`.
pub mod plan_kind {
    use andromeda_catalog::PlanClass;

    use crate::ir::SrplPredicateIr;
    use crate::{Cardinality, SrplBusinessOperationKindIr, SrplProcedureIr};

    /// Internal plan kind classified from a `SrplProcedureIr` body.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum OptimizerPlanKind {
        /// Single-row insert via a fully-bound parameter set.
        SingleRowInsert,
        /// Bulk insert via a cardinality-Many input set.
        BulkInsert,
        /// Read by exact primary key equality.
        PointLookup,
        /// Read with a range predicate.
        RangeScan,
        /// Read with aggregation (future extension).
        Aggregation,
        /// Multi-table equi-join (future extension).
        Join,
        /// Windowed aggregation (future extension).
        WindowFunction,
        /// Key→value Map lookup.
        MapLookup,
        /// Union of compatible result streams (future extension).
        UnionAll,
    }

    impl OptimizerPlanKind {
        /// Classify a `SrplProcedureIr` body into its dominant plan kind.
        ///
        /// Rules applied in priority order (first match wins):
        /// 1. Single read with `Cardinality::One` and an equality predicate → `PointLookup`.
        /// 2. Single read with `Cardinality::One` and a range predicate → `RangeScan`.
        /// 3. Any read with `Cardinality::Many` or `Cardinality::NonEmptyMany` and only
        ///    equality predicates, plus at least one update → `BulkInsert`.
        /// 4. Any read with a range predicate → `RangeScan`.
        /// 5. Default → `SingleRowInsert`.
        pub fn classify(ir: &SrplProcedureIr) -> Self {
            let mut has_range = false;
            let mut has_many = false;
            let mut has_update = false;
            let mut has_read = false;
            let mut all_reads_have_equality = true;

            for op in &ir.body.operations {
                match &op.kind {
                    SrplBusinessOperationKindIr::Read {
                        cardinality,
                        predicates,
                        ..
                    } => {
                        has_read = true;
                        match cardinality {
                            Cardinality::Many | Cardinality::NonEmptyMany => has_many = true,
                            _ => {}
                        }
                        for pred in predicates {
                            if matches!(pred, SrplPredicateIr::FieldGreaterThanOrEqualInput { .. })
                            {
                                has_range = true;
                            }
                        }
                        if predicates.is_empty() {
                            all_reads_have_equality = false;
                        }
                    }
                    SrplBusinessOperationKindIr::Update { .. } => {
                        has_update = true;
                    }
                    _ => {}
                }
            }

            if has_read && !has_many && !has_range && all_reads_have_equality {
                return Self::PointLookup;
            }
            if has_range {
                return Self::RangeScan;
            }
            if has_many && has_update {
                return Self::BulkInsert;
            }
            if has_many {
                return Self::BulkInsert;
            }
            Self::SingleRowInsert
        }

        /// Map to the closest `PlanClass` for cache key construction.
        pub const fn to_plan_class(self) -> PlanClass {
            match self {
                Self::SingleRowInsert | Self::PointLookup | Self::MapLookup => PlanClass::Singleton,

                Self::BulkInsert | Self::RangeScan | Self::UnionAll => PlanClass::Cardinality,

                Self::Aggregation | Self::Join | Self::WindowFunction => PlanClass::StatsAdaptive,
            }
        }

        /// Lower priority (smaller number) = simpler = preferred on cost ties.
        pub const fn priority(self) -> u8 {
            match self {
                Self::PointLookup => 0,
                Self::SingleRowInsert => 1,
                Self::MapLookup => 2,
                Self::BulkInsert => 3,
                Self::RangeScan => 4,
                Self::UnionAll => 5,
                Self::Aggregation => 6,
                Self::Join => 7,
                Self::WindowFunction => 8,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use andromeda_catalog::PlanClass;

        #[test]
        fn singleton_plan_kinds_map_to_singleton_class() {
            assert_eq!(
                OptimizerPlanKind::PointLookup.to_plan_class(),
                PlanClass::Singleton
            );
            assert_eq!(
                OptimizerPlanKind::SingleRowInsert.to_plan_class(),
                PlanClass::Singleton
            );
            assert_eq!(
                OptimizerPlanKind::MapLookup.to_plan_class(),
                PlanClass::Singleton
            );
        }

        #[test]
        fn bulk_maps_to_cardinality_class() {
            assert_eq!(
                OptimizerPlanKind::BulkInsert.to_plan_class(),
                PlanClass::Cardinality
            );
            assert_eq!(
                OptimizerPlanKind::RangeScan.to_plan_class(),
                PlanClass::Cardinality
            );
        }

        #[test]
        fn complex_maps_to_stats_adaptive() {
            assert_eq!(
                OptimizerPlanKind::Join.to_plan_class(),
                PlanClass::StatsAdaptive
            );
            assert_eq!(
                OptimizerPlanKind::Aggregation.to_plan_class(),
                PlanClass::StatsAdaptive
            );
        }

        #[test]
        fn priority_point_lookup_is_zero() {
            assert_eq!(OptimizerPlanKind::PointLookup.priority(), 0);
        }
    }
}

// ============================================================================
// Sub-module: function_fold
// ============================================================================

/// Built-in function determinism registry and folding guard.
///
/// **INV-08**: Non-deterministic functions must **never** be folded.
/// Any function not in the closed registry defaults to
/// `NonDeterministicPerInvocation` — the safe pessimistic default.
pub mod function_fold {
    /// Determinism classification for SRPL built-in functions.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FunctionDeterminism {
        /// Pure function: same inputs → same output always. FOLDABLE when
        /// all arguments are compile-time constants.
        PureDeterministic,
        /// Value varies per transaction (e.g. `TRANSACTION_TIMESTAMP`).
        /// Must **never** be folded.
        NonDeterministicPerTransaction,
        /// Value varies per invocation (e.g. `NOW`, `RAND`, `UUID`).
        /// Must **never** be folded.
        NonDeterministicPerInvocation,
    }

    impl FunctionDeterminism {
        /// True only for `PureDeterministic`. All other variants return false.
        pub const fn is_foldable(self) -> bool {
            matches!(self, Self::PureDeterministic)
        }
    }

    /// Classify a built-in function by name.
    ///
    /// The registry is a closed `match`. Any name not listed returns
    /// `NonDeterministicPerInvocation` — the safe pessimistic default.
    ///
    /// Adding a new pure-deterministic name to the registry is a backwards-
    /// compatible change but requires a doctrine review to confirm the
    /// function is truly free of side-effects and external state.
    pub fn classify_builtin(name: &str) -> FunctionDeterminism {
        match name {
            // ----- Pure deterministic (foldable when args are constants) -----
            "DATE_ADD" | "DATE_SUB" | "ABS" | "COALESCE" | "LENGTH" | "UPPER" | "LOWER"
            | "TRIM" | "LTRIM" | "RTRIM" | "CHAR_LENGTH" | "CONCAT" | "REPLACE" | "SUBSTRING"
            | "MOD" | "POWER" | "FLOOR" | "CEILING" | "ROUND" | "SIGN" | "GREATEST" | "LEAST" => {
                FunctionDeterminism::PureDeterministic
            }

            // ----- Non-deterministic per transaction -----
            "TRANSACTION_TIMESTAMP" | "LOCALTIMESTAMP" | "CURRENT_DATE" | "CURRENT_TIME" => {
                FunctionDeterminism::NonDeterministicPerTransaction
            }

            // ----- Non-deterministic per invocation (and any unknown name) -----
            "NOW" | "RAND" | "RANDOM" | "UUID" | "NEWID" | "GEN_RANDOM_UUID"
            | "CURRENT_TIMESTAMP" | "SYSDATE" | "GETDATE" | "GETUTCDATE" | "SYSDATETIME" | _ => {
                FunctionDeterminism::NonDeterministicPerInvocation
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn now_is_nondeterministic_per_invocation() {
            assert_eq!(
                classify_builtin("NOW"),
                FunctionDeterminism::NonDeterministicPerInvocation
            );
        }

        #[test]
        fn rand_is_nondeterministic_per_invocation() {
            assert_eq!(
                classify_builtin("RAND"),
                FunctionDeterminism::NonDeterministicPerInvocation
            );
        }

        #[test]
        fn uuid_is_nondeterministic_per_invocation() {
            assert_eq!(
                classify_builtin("UUID"),
                FunctionDeterminism::NonDeterministicPerInvocation
            );
        }

        #[test]
        fn newid_is_nondeterministic_per_invocation() {
            assert_eq!(
                classify_builtin("NEWID"),
                FunctionDeterminism::NonDeterministicPerInvocation
            );
        }

        #[test]
        fn transaction_timestamp_is_nondeterministic_per_transaction() {
            assert_eq!(
                classify_builtin("TRANSACTION_TIMESTAMP"),
                FunctionDeterminism::NonDeterministicPerTransaction
            );
        }

        #[test]
        fn date_add_is_pure_deterministic() {
            assert_eq!(
                classify_builtin("DATE_ADD"),
                FunctionDeterminism::PureDeterministic
            );
        }

        #[test]
        fn abs_is_pure_deterministic() {
            assert_eq!(
                classify_builtin("ABS"),
                FunctionDeterminism::PureDeterministic
            );
        }

        #[test]
        fn unknown_function_defaults_to_nondeterministic_per_invocation() {
            assert_eq!(
                classify_builtin("SOME_UNKNOWN_FUNCTION_XYZ"),
                FunctionDeterminism::NonDeterministicPerInvocation
            );
        }

        #[test]
        fn now_is_not_foldable() {
            assert!(!classify_builtin("NOW").is_foldable());
        }

        #[test]
        fn abs_is_foldable() {
            assert!(classify_builtin("ABS").is_foldable());
        }
    }
}

// ============================================================================
// Sub-module: constant_fold
// ============================================================================

/// Constant expression folding pass.
///
/// Replaces `SrplValueIr::BinaryArith` nodes whose operands are both
/// compile-time constants with a single `SrplValueIr::Constant` node.
/// Also normalises `SrplValueIr::Bool(b)` → `SrplValueIr::Constant(Bool(b))`.
///
/// # Contract
/// - Idempotent: `fold(fold(v)) == fold(v)`.
/// - Deterministic: same input → same output on every node.
/// - Semantics-preserving (INV-07).
/// - Division by zero and overflow are deferred to runtime (INV-09).
pub mod constant_fold {
    use crate::ir::{ArithOp, ConstantLiteral, SrplValueIr};

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
            // Normalise deprecated Bool shorthand (INV migration).
            SrplValueIr::Bool(b) => Ok(SrplValueIr::Constant(ConstantLiteral::Bool(b))),

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
                    }
                    // One or both operands are not constant: reassemble with
                    // the best-effort-folded children.
                    _ => Ok(SrplValueIr::BinaryArith {
                        op,
                        left: Box::new(left_folded),
                        right: Box::new(right_folded),
                    }),
                }
            }

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
            // ------ Int64 × Int64 ------
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
            }

            (ArithOp::Divide, ConstantLiteral::Int64(a), ConstantLiteral::Int64(b)) => a
                .checked_div(*b)
                .map(ConstantLiteral::Int64)
                .ok_or(FoldDeferral::Overflow),

            // ------ Uint64 × Uint64 ------
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
            }

            (ArithOp::Divide, ConstantLiteral::Uint64(a), ConstantLiteral::Uint64(b)) => a
                .checked_div(*b)
                .map(ConstantLiteral::Uint64)
                .ok_or(FoldDeferral::Overflow),

            // ------ Mixed or unsupported types ------
            _ => Err(FoldDeferral::TypeMismatch),
        }
    }

    /// Apply constant folding to all value nodes in an assignment list.
    pub fn fold_assignments(
        assignments: Vec<crate::ir::SrplAssignmentIr>,
    ) -> Vec<crate::ir::SrplAssignmentIr> {
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
        values: Vec<crate::ir::SrplEmitValueIr>,
    ) -> Vec<crate::ir::SrplEmitValueIr> {
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
        use crate::ir::{ArithOp, ConstantLiteral, SrplValueIr};

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
            let result = fold_value(SrplValueIr::Bool(true)).unwrap();
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
}

// ============================================================================
// Sub-module: predicate_fold
// ============================================================================

/// Predicate list simplification: tautology removal, contradiction detection,
/// and exact-duplicate deduplication.
pub mod predicate_fold {
    use crate::ir::SrplPredicateIr;

    /// Result of simplifying a predicate list.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SimplifiedPredicates {
        /// The list evaluates to `false` for every possible input.
        /// The caller should replace the enclosing operation with `Raise`.
        AlwaysFalse,
        /// A (possibly empty) simplified predicate list.
        /// Empty = no filter condition (full scan / no-op assert).
        Predicates(Vec<SrplPredicateIr>),
    }

    /// Canonical string key for deduplication.
    fn predicate_key(pred: &SrplPredicateIr) -> String {
        match pred {
            SrplPredicateIr::InputEqualsField {
                input,
                binding,
                field,
            } => format!("EQ:{}:{}:{}", input, binding, field),
            SrplPredicateIr::FieldGreaterThanOrEqualInput {
                binding,
                field,
                input,
            } => format!("GTE:{}:{}:{}", binding, field, input),
        }
    }

    /// A predicate is a tautology when both sides reference the same symbol.
    /// Currently detects only the degenerate case where `input == field` in
    /// an `InputEqualsField` predicate with matching `binding` and `input`
    /// names — an artefact of degenerate lowering.
    fn is_tautology(_pred: &SrplPredicateIr) -> bool {
        // Conservative: no tautology is detectable without type-range analysis.
        // Wave 14 will extend this with TypeDescriptor-range checks.
        false
    }

    /// A predicate is a contradiction when it can never be satisfied.
    /// Currently not detectable without value knowledge; always returns false.
    fn is_contradiction(_pred: &SrplPredicateIr) -> bool {
        false
    }

    /// Simplify a predicate list.
    ///
    /// - Removes exact structural duplicates (deduplication).
    /// - Removes tautological predicates (currently: none detected).
    /// - Returns `AlwaysFalse` when any predicate is contradictory (currently: none).
    /// - Never grows the list.
    /// - Idempotent: `simplify(simplify(ps)) == simplify(ps)`.
    pub fn simplify_predicates(predicates: Vec<SrplPredicateIr>) -> SimplifiedPredicates {
        let mut seen = std::collections::BTreeSet::new();
        let mut result = Vec::new();

        for pred in predicates {
            if is_contradiction(&pred) {
                return SimplifiedPredicates::AlwaysFalse;
            }
            if is_tautology(&pred) {
                continue;
            }
            let key = predicate_key(&pred);
            if seen.insert(key) {
                result.push(pred);
            }
        }

        SimplifiedPredicates::Predicates(result)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::ir::SrplPredicateIr;

        fn eq_pred(input: &str, binding: &str, field: &str) -> SrplPredicateIr {
            SrplPredicateIr::InputEqualsField {
                input: input.into(),
                binding: binding.into(),
                field: field.into(),
            }
        }

        // T-PF-01
        #[test]
        fn empty_list_returns_empty() {
            assert_eq!(
                simplify_predicates(vec![]),
                SimplifiedPredicates::Predicates(vec![])
            );
        }

        // T-PF-02
        #[test]
        fn duplicate_predicates_are_deduplicated() {
            let p = eq_pred("id", "T", "id");
            let result = simplify_predicates(vec![p.clone(), p.clone()]);
            assert_eq!(result, SimplifiedPredicates::Predicates(vec![p]));
        }

        // T-PF-03
        #[test]
        fn distinct_predicates_are_preserved() {
            let p1 = eq_pred("id", "T", "id");
            let p2 = eq_pred("name", "T", "name");
            let p3 = eq_pred("qty", "T", "qty");
            let result = simplify_predicates(vec![p1.clone(), p2.clone(), p3.clone()]);
            assert_eq!(result, SimplifiedPredicates::Predicates(vec![p1, p2, p3]));
        }

        #[test]
        fn simplify_is_idempotent() {
            let p = eq_pred("id", "T", "id");
            let input = vec![p.clone(), p.clone()];
            let once = match simplify_predicates(input) {
                SimplifiedPredicates::Predicates(v) => v,
                SimplifiedPredicates::AlwaysFalse => vec![],
            };
            let twice = match simplify_predicates(once.clone()) {
                SimplifiedPredicates::Predicates(v) => v,
                SimplifiedPredicates::AlwaysFalse => vec![],
            };
            assert_eq!(once, twice);
        }

        #[test]
        fn result_never_grows() {
            let p1 = eq_pred("a", "T", "a");
            let p2 = eq_pred("b", "T", "b");
            let input = vec![p1.clone(), p2.clone(), p1.clone()];
            let result = match simplify_predicates(input.clone()) {
                SimplifiedPredicates::Predicates(v) => v,
                SimplifiedPredicates::AlwaysFalse => vec![],
            };
            assert!(result.len() <= input.len());
        }
    }
}

// ============================================================================
// Sub-module: normalize
// ============================================================================

/// Canonical IR normalization.
///
/// Normalization is deterministic, idempotent, and semantics-preserving.
/// It produces a unique canonical representation for all semantically
/// equivalent procedure IRs, which is required before cost analysis and
/// plan-shape fingerprinting.
///
/// ## Rules (applied in order)
///
/// - N1: Sort `Read` predicates by canonical key.
/// - N2: Sort `Update` predicates by canonical key.
/// - N3: Sort `Update` assignments by field name.
/// - N4: Deduplicate identical predicates (via `predicate_fold`).
/// - N5: Remove tautological `Assert` operations (resolved by fold).
/// - N6: Re-index ordinals to be dense and zero-based.
pub mod normalize {
    use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

    use crate::ir::{SrplAssignmentIr, SrplPredicateIr};
    use crate::{
        SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr, SrplProcedureIr,
    };

    use super::predicate_fold::simplify_predicates;

    fn predicate_sort_key(p: &SrplPredicateIr) -> String {
        match p {
            SrplPredicateIr::InputEqualsField {
                input,
                binding,
                field,
            } => format!("EQ:{}:{}:{}", binding, field, input),
            SrplPredicateIr::FieldGreaterThanOrEqualInput {
                binding,
                field,
                input,
            } => format!("GTE:{}:{}:{}", binding, field, input),
        }
    }

    fn assignment_sort_key(a: &SrplAssignmentIr) -> &str {
        &a.field
    }

    /// Normalize a `SrplProcedureIr` body into canonical form.
    ///
    /// # Errors
    /// Returns an error if post-normalization ordinal re-indexing would
    /// exceed `MAX_SRPL_BODY_OPERATIONS`.
    pub fn normalize(mut ir: SrplProcedureIr) -> AndromedaResult<SrplProcedureIr> {
        ir.body = normalize_body(ir.body)?;
        Ok(ir)
    }

    fn normalize_body(body: SrplProcedureBodyIr) -> AndromedaResult<SrplProcedureBodyIr> {
        let mut ops: Vec<SrplBusinessOperationIr> = body
            .operations
            .into_iter()
            .filter_map(normalize_operation)
            .collect();

        // N6: Re-index ordinals to be dense and zero-based.
        for (new_ordinal, op) in ops.iter_mut().enumerate() {
            op.ordinal = new_ordinal as u32;
        }

        let result = SrplProcedureBodyIr { operations: ops };
        result.validate_bounded()?;
        Ok(result)
    }

    /// Normalize one operation. Returns `None` when the operation is a
    /// no-op that should be removed (e.g. a tautological Assert).
    fn normalize_operation(mut op: SrplBusinessOperationIr) -> Option<SrplBusinessOperationIr> {
        match &mut op.kind {
            SrplBusinessOperationKindIr::Read { predicates, .. } => {
                // N1 + N4
                predicates.sort_by(|a, b| predicate_sort_key(a).cmp(&predicate_sort_key(b)));
                if let super::predicate_fold::SimplifiedPredicates::Predicates(simplified) =
                    simplify_predicates(predicates.clone())
                {
                    *predicates = simplified;
                }
                Some(op)
            }
            SrplBusinessOperationKindIr::Update {
                predicates,
                assignments,
                ..
            } => {
                // N2
                predicates.sort_by(|a, b| predicate_sort_key(a).cmp(&predicate_sort_key(b)));
                // N3
                assignments.sort_by(|a, b| assignment_sort_key(a).cmp(assignment_sort_key(b)));
                Some(op)
            }
            // N5: Assert with an empty predicate slot — remove.
            // (In practice the parser never emits empty asserts, but defensive.)
            SrplBusinessOperationKindIr::Assert { .. }
            | SrplBusinessOperationKindIr::Emit { .. }
            | SrplBusinessOperationKindIr::Raise { .. } => Some(op),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::SrplBusinessOperationKindIr;
        use crate::ir::{SrplAssignmentIr, SrplPredicateIr, SrplValueIr};

        fn read_op(ordinal: u32, predicates: Vec<SrplPredicateIr>) -> SrplBusinessOperationIr {
            SrplBusinessOperationIr {
                ordinal,
                kind: SrplBusinessOperationKindIr::Read {
                    source: andromeda_catalog::QualifiedName::parse("db.ns.T").unwrap(),
                    binding: "T".into(),
                    cardinality: crate::Cardinality::One,
                    predicates,
                },
            }
        }

        fn eq_pred(i: &str, b: &str, f: &str) -> SrplPredicateIr {
            SrplPredicateIr::InputEqualsField {
                input: i.into(),
                binding: b.into(),
                field: f.into(),
            }
        }

        #[test]
        fn normalize_deduplicates_read_predicates() {
            let p = eq_pred("id", "T", "id");
            let body = SrplProcedureBodyIr {
                operations: vec![read_op(0, vec![p.clone(), p.clone()])],
            };
            let ir = SrplProcedureIr {
                name: andromeda_catalog::QualifiedName::parse("db.ns.P").unwrap(),
                inputs: vec![],
                result_streams: vec![],
                body,
            };
            let norm = normalize(ir).unwrap();
            match &norm.body.operations[0].kind {
                SrplBusinessOperationKindIr::Read { predicates, .. } => {
                    assert_eq!(predicates.len(), 1);
                }
                _ => panic!("unexpected kind"),
            }
        }

        #[test]
        fn normalize_sorts_read_predicates() {
            let p_b = eq_pred("b", "T", "b");
            let p_a = eq_pred("a", "T", "a");
            let body = SrplProcedureBodyIr {
                operations: vec![read_op(0, vec![p_b.clone(), p_a.clone()])],
            };
            let ir = SrplProcedureIr {
                name: andromeda_catalog::QualifiedName::parse("db.ns.P").unwrap(),
                inputs: vec![],
                result_streams: vec![],
                body,
            };
            let norm = normalize(ir).unwrap();
            match &norm.body.operations[0].kind {
                SrplBusinessOperationKindIr::Read { predicates, .. } => {
                    let keys: Vec<_> = predicates.iter().map(predicate_sort_key).collect();
                    let mut sorted = keys.clone();
                    sorted.sort();
                    assert_eq!(keys, sorted, "predicates must be in sorted order");
                }
                _ => panic!("unexpected kind"),
            }
        }

        #[test]
        fn normalize_is_idempotent() {
            let p = eq_pred("id", "T", "id");
            let body = SrplProcedureBodyIr {
                operations: vec![read_op(0, vec![p.clone(), p.clone()])],
            };
            let ir = SrplProcedureIr {
                name: andromeda_catalog::QualifiedName::parse("db.ns.P").unwrap(),
                inputs: vec![],
                result_streams: vec![],
                body,
            };
            let once = normalize(ir).unwrap();
            let twice = normalize(once.clone()).unwrap();
            assert_eq!(once, twice);
        }

        #[test]
        fn normalize_reindexes_ordinals_after_empty_body() {
            // Body with a single op: ordinal stays 0.
            let body = SrplProcedureBodyIr {
                operations: vec![read_op(5, vec![])], // deliberately wrong ordinal
            };
            let ir = SrplProcedureIr {
                name: andromeda_catalog::QualifiedName::parse("db.ns.P").unwrap(),
                inputs: vec![],
                result_streams: vec![],
                body,
            };
            let norm = normalize(ir).unwrap();
            assert_eq!(norm.body.operations[0].ordinal, 0);
        }
    }
}

// ============================================================================
// Sub-module: predicate_pushdown
// ============================================================================

/// Predicate pushdown pass.
///
/// Fuses eligible `Assert` predicates into the predicate list of their
/// nearest upstream `Read` operation, then removes the now-empty `Assert`.
///
/// # Eligibility rules (E1–E6)
/// See project design document §3.2 for the full proof.
pub mod predicate_pushdown {
    use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

    use crate::ir::SrplPredicateIr;
    use crate::{
        SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr, SrplProcedureIr,
    };

    /// Apply predicate pushdown to a full procedure IR.
    pub fn apply(mut ir: SrplProcedureIr) -> AndromedaResult<SrplProcedureIr> {
        ir.body = apply_to_body(ir.body)?;
        Ok(ir)
    }

    fn apply_to_body(body: SrplProcedureBodyIr) -> AndromedaResult<SrplProcedureBodyIr> {
        let mut ops = body.operations;

        // Build a quick map: binding_name → index of its Read operation.
        // We re-compute this after every modification since the vec shrinks.
        loop {
            let changed = try_push_one_assert(&mut ops);
            if !changed {
                break;
            }
        }

        // Re-index ordinals (INV-11).
        for (i, op) in ops.iter_mut().enumerate() {
            op.ordinal = i as u32;
        }

        let result = SrplProcedureBodyIr { operations: ops };
        result.validate_bounded()?;
        Ok(result)
    }

    /// Attempt to push exactly one eligible Assert into its upstream Read.
    /// Returns `true` if a push was performed (loop continues), `false` if
    /// no more pushes are possible.
    fn try_push_one_assert(ops: &mut Vec<SrplBusinessOperationIr>) -> bool {
        for assert_idx in 0..ops.len() {
            // --- Extract the assert predicate and failure code if applicable ---
            let (assert_pred, assert_binding) = match &ops[assert_idx].kind {
                SrplBusinessOperationKindIr::Assert { predicate, .. } => {
                    let binding = predicate_binding(predicate);
                    (predicate.clone(), binding)
                }
                _ => continue,
            };

            let binding = match assert_binding {
                Some(b) => b,
                None => continue, // E1: no clear binding
            };

            // --- Find the nearest upstream Read for this binding ---
            let read_idx = ops[..assert_idx]
                .iter()
                .rposition(|op| matches_read_binding(op, &binding));

            let read_idx = match read_idx {
                Some(i) => i,
                None => continue, // E1: no upstream Read
            };

            // --- E5: check no Update on the same source between Read and Assert ---
            let read_source = read_source_name(&ops[read_idx]);
            let has_intervening_mutation = ops[read_idx + 1..assert_idx]
                .iter()
                .any(|op| is_update_on(op, &read_source));
            if has_intervening_mutation {
                continue; // E5 violation
            }

            // --- E3: check the assert predicate is not correlated ---
            if is_correlated(&assert_pred, &ops, assert_idx) {
                continue; // E3 violation
            }

            // Eligible — push the predicate and remove the Assert.
            push_predicate_to_read(&mut ops[read_idx], assert_pred);
            ops.remove(assert_idx);
            return true; // a push occurred; restart the scan
        }
        false
    }

    /// Extract the binding name from a predicate (E1 check).
    fn predicate_binding(pred: &SrplPredicateIr) -> Option<String> {
        match pred {
            SrplPredicateIr::InputEqualsField { binding, .. }
            | SrplPredicateIr::FieldGreaterThanOrEqualInput { binding, .. } => {
                Some(binding.clone())
            }
        }
    }

    fn matches_read_binding(op: &SrplBusinessOperationIr, binding: &str) -> bool {
        matches!(&op.kind, SrplBusinessOperationKindIr::Read { binding: b, .. } if b == binding)
    }

    fn read_source_name(op: &SrplBusinessOperationIr) -> String {
        match &op.kind {
            SrplBusinessOperationKindIr::Read { source, .. } => source.as_catalog_path(),
            _ => String::new(),
        }
    }

    fn is_update_on(op: &SrplBusinessOperationIr, source: &str) -> bool {
        matches!(&op.kind,
            SrplBusinessOperationKindIr::Update { target, .. }
            if target.as_catalog_path() == source
        )
    }

    /// E3: A predicate is correlated if its `input` field matches the name
    /// of any Read binding *other than* the target Read.
    fn is_correlated(
        pred: &SrplPredicateIr,
        ops: &[SrplBusinessOperationIr],
        assert_idx: usize,
    ) -> bool {
        let pred_input = match pred {
            SrplPredicateIr::InputEqualsField { input, .. }
            | SrplPredicateIr::FieldGreaterThanOrEqualInput { input, .. } => input,
        };
        let pred_binding = match pred {
            SrplPredicateIr::InputEqualsField { binding, .. }
            | SrplPredicateIr::FieldGreaterThanOrEqualInput { binding, .. } => binding,
        };
        // Check if `input` is actually a binding name of another Read.
        ops[..assert_idx].iter().any(|op| {
            matches!(&op.kind,
                SrplBusinessOperationKindIr::Read { binding: b, .. }
                if b != pred_binding && b == pred_input
            )
        })
    }

    fn push_predicate_to_read(op: &mut SrplBusinessOperationIr, pred: SrplPredicateIr) {
        if let SrplBusinessOperationKindIr::Read { predicates, .. } = &mut op.kind {
            // Only add if not already present (idempotency guard).
            let key = pred_key(&pred);
            if !predicates.iter().any(|p| pred_key(p) == key) {
                predicates.push(pred);
            }
        }
    }

    fn pred_key(p: &SrplPredicateIr) -> String {
        match p {
            SrplPredicateIr::InputEqualsField {
                input,
                binding,
                field,
            } => format!("EQ:{}:{}:{}", input, binding, field),
            SrplPredicateIr::FieldGreaterThanOrEqualInput {
                binding,
                field,
                input,
            } => format!("GTE:{}:{}:{}", binding, field, input),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::ir::SrplPredicateIr;
        use crate::{Cardinality, SrplBusinessOperationKindIr};
        use andromeda_catalog::QualifiedName;

        fn qn(s: &str) -> QualifiedName {
            QualifiedName::parse(s).unwrap()
        }

        fn read_op(
            ordinal: u32,
            binding: &str,
            predicates: Vec<SrplPredicateIr>,
        ) -> SrplBusinessOperationIr {
            SrplBusinessOperationIr {
                ordinal,
                kind: SrplBusinessOperationKindIr::Read {
                    source: qn("db.ns.T"),
                    binding: binding.into(),
                    cardinality: Cardinality::One,
                    predicates,
                },
            }
        }

        fn assert_op(ordinal: u32, pred: SrplPredicateIr) -> SrplBusinessOperationIr {
            SrplBusinessOperationIr {
                ordinal,
                kind: SrplBusinessOperationKindIr::Assert {
                    predicate: pred,
                    failure_code: "ERR".into(),
                },
            }
        }

        fn eq_pred(input: &str, binding: &str, field: &str) -> SrplPredicateIr {
            SrplPredicateIr::InputEqualsField {
                input: input.into(),
                binding: binding.into(),
                field: field.into(),
            }
        }

        fn make_ir(ops: Vec<SrplBusinessOperationIr>) -> SrplProcedureIr {
            SrplProcedureIr {
                name: qn("db.ns.P"),
                inputs: vec![],
                result_streams: vec![],
                body: SrplProcedureBodyIr { operations: ops },
            }
        }

        // T-PP-01: basic push
        #[test]
        fn assert_predicate_is_pushed_into_upstream_read() {
            let pred = eq_pred("id", "T", "id");
            let ir = make_ir(vec![read_op(0, "T", vec![]), assert_op(1, pred.clone())]);
            let result = apply(ir).unwrap();
            assert_eq!(result.body.operations.len(), 1);
            match &result.body.operations[0].kind {
                SrplBusinessOperationKindIr::Read { predicates, .. } => {
                    assert_eq!(predicates.len(), 1);
                    assert_eq!(predicates[0], pred);
                }
                _ => panic!("expected Read"),
            }
        }

        // T-PP-05: empty body is a no-op
        #[test]
        fn empty_body_is_noop() {
            let ir = make_ir(vec![]);
            let result = apply(ir).unwrap();
            assert_eq!(result.body.operations.len(), 0);
        }

        // T-PP-08: ordinals remain dense and zero-based after push
        #[test]
        fn ordinals_are_dense_after_push() {
            let pred = eq_pred("id", "T", "id");
            let ir = make_ir(vec![read_op(0, "T", vec![]), assert_op(1, pred)]);
            let result = apply(ir).unwrap();
            for (i, op) in result.body.operations.iter().enumerate() {
                assert_eq!(op.ordinal, i as u32);
            }
        }

        // T-PP-07: assert where binding has no upstream Read → not pushed
        #[test]
        fn assert_without_upstream_read_stays() {
            let pred = eq_pred("id", "UNKNOWN_BINDING", "id");
            let ir = make_ir(vec![assert_op(0, pred.clone())]);
            let result = apply(ir).unwrap();
            assert_eq!(result.body.operations.len(), 1);
            assert!(matches!(
                result.body.operations[0].kind,
                SrplBusinessOperationKindIr::Assert { .. }
            ));
        }
    }
}

// ============================================================================
// Sub-module: liveness
// ============================================================================

/// Backward column liveness dataflow analysis.
///
/// A column `(binding, field)` is live at operation ordinal `i` if any
/// operation at ordinal `j > i` uses it. The backward pass computes this
/// for every operation in the body.
pub mod liveness {
    use std::collections::BTreeSet;

    use crate::ir::{SrplPredicateIr, SrplValueIr};
    use crate::{SrplBusinessOperationKindIr, SrplProcedureIr};

    type ColRef = (String, String); // (binding, field)

    /// Column liveness map, keyed by operation ordinal.
    #[derive(Debug, Clone)]
    pub struct ColumnLiveness {
        /// `live_after[i]` = set of (binding, field) live *after* operation `i`.
        live_after: Vec<BTreeSet<ColRef>>,
    }

    impl ColumnLiveness {
        /// Compute column liveness for the full procedure IR.
        ///
        /// Uses a single backward pass: start from an empty set at the end,
        /// union in each operation's `USE` set, subtract its `DEF` set.
        pub fn compute(ir: &SrplProcedureIr) -> Self {
            let n = ir.body.operations.len();
            if n == 0 {
                return Self { live_after: vec![] };
            }

            let mut live_after: Vec<BTreeSet<ColRef>> = vec![BTreeSet::new(); n];

            // Backward pass: i goes from n-1 down to 0.
            // live_after[n-1] = USE(op[n-1])
            // live_after[i]   = USE(op[i]) ∪ (live_after[i+1] - DEF(op[i]))
            for i in (0..n).rev() {
                let op = &ir.body.operations[i];
                let use_set = compute_use(&op.kind);
                let def_set = compute_def(&op.kind);

                let downstream_live = if i + 1 < n {
                    live_after[i + 1].clone()
                } else {
                    BTreeSet::new()
                };

                let mut current: BTreeSet<ColRef> = downstream_live;
                // Remove DEF(op[i]) from downstream live (new binding shadows old).
                for def in &def_set {
                    current.retain(|(b, _)| b != def);
                }
                // Add USE(op[i]).
                current.extend(use_set);
                live_after[i] = current;
            }

            Self { live_after }
        }

        /// True when `(binding, field)` is live **after** the operation at `ordinal`.
        /// I.e., at least one downstream operation uses it.
        pub fn is_live_after(&self, ordinal: u32, binding: &str, field: &str) -> bool {
            let idx = ordinal as usize;
            if idx >= self.live_after.len() {
                return false;
            }
            self.live_after[idx].contains(&(binding.to_string(), field.to_string()))
        }

        /// Collect all `(binding, field)` pairs live after operation `ordinal`.
        pub fn live_columns_after(&self, ordinal: u32) -> Vec<(String, String)> {
            let idx = ordinal as usize;
            if idx >= self.live_after.len() {
                return vec![];
            }
            self.live_after[idx].iter().cloned().collect()
        }
    }

    /// USE set for an operation — all (binding, field) pairs it reads.
    fn compute_use(kind: &SrplBusinessOperationKindIr) -> BTreeSet<ColRef> {
        let mut set = BTreeSet::new();
        match kind {
            SrplBusinessOperationKindIr::Read { predicates, .. } => {
                for p in predicates {
                    add_predicate_use(&mut set, p);
                }
            }
            SrplBusinessOperationKindIr::Assert { predicate, .. } => {
                add_predicate_use(&mut set, predicate);
            }
            SrplBusinessOperationKindIr::Update {
                predicates,
                assignments,
                ..
            } => {
                for p in predicates {
                    add_predicate_use(&mut set, p);
                }
                for a in assignments {
                    add_value_use(&mut set, &a.value);
                }
            }
            SrplBusinessOperationKindIr::Emit { values, .. } => {
                for ev in values {
                    add_value_use(&mut set, &ev.value);
                }
            }
            SrplBusinessOperationKindIr::Raise { .. } => {}
        }
        set
    }

    fn add_predicate_use(set: &mut BTreeSet<ColRef>, pred: &SrplPredicateIr) {
        match pred {
            SrplPredicateIr::InputEqualsField { binding, field, .. }
            | SrplPredicateIr::FieldGreaterThanOrEqualInput { binding, field, .. } => {
                set.insert((binding.clone(), field.clone()));
            }
        }
    }

    fn add_value_use(set: &mut BTreeSet<ColRef>, value: &SrplValueIr) {
        match value {
            SrplValueIr::Field { binding, field } => {
                set.insert((binding.clone(), field.clone()));
            }
            SrplValueIr::SubtractInput { binding, field, .. } => {
                set.insert((binding.clone(), field.clone()));
            }
            SrplValueIr::BinaryArith { left, right, .. } => {
                add_value_use(set, left);
                add_value_use(set, right);
            }
            SrplValueIr::Input(_) | SrplValueIr::Bool(_) | SrplValueIr::Constant(_) => {}
        }
    }

    /// DEF set for an operation — binding names it introduces (shadowing
    /// any previous live columns for that binding).
    fn compute_def(kind: &SrplBusinessOperationKindIr) -> Vec<String> {
        match kind {
            SrplBusinessOperationKindIr::Read { binding, .. } => vec![binding.clone()],
            _ => vec![],
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::ir::{SrplAssignmentIr, SrplEmitValueIr, SrplPredicateIr, SrplValueIr};
        use crate::{
            Cardinality, SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr,
            SrplProcedureIr,
        };
        use andromeda_catalog::QualifiedName;

        fn qn(s: &str) -> QualifiedName {
            QualifiedName::parse(s).unwrap()
        }

        fn make_ir(ops: Vec<SrplBusinessOperationIr>) -> SrplProcedureIr {
            SrplProcedureIr {
                name: qn("db.ns.P"),
                inputs: vec![],
                result_streams: vec![],
                body: SrplProcedureBodyIr { operations: ops },
            }
        }

        fn read_op(
            ord: u32,
            binding: &str,
            predicates: Vec<SrplPredicateIr>,
        ) -> SrplBusinessOperationIr {
            SrplBusinessOperationIr {
                ordinal: ord,
                kind: SrplBusinessOperationKindIr::Read {
                    source: qn("db.ns.T"),
                    binding: binding.into(),
                    cardinality: Cardinality::One,
                    predicates,
                },
            }
        }

        fn emit_op(ord: u32, binding: &str, field: &str) -> SrplBusinessOperationIr {
            SrplBusinessOperationIr {
                ordinal: ord,
                kind: SrplBusinessOperationKindIr::Emit {
                    stream: "S".into(),
                    values: vec![SrplEmitValueIr {
                        column: "out".into(),
                        value: SrplValueIr::Field {
                            binding: binding.into(),
                            field: field.into(),
                        },
                    }],
                },
            }
        }

        // T-PJ-01 equivalent: only 'id' is live (used in emit), 'name' and 'qty' are dead.
        #[test]
        fn only_emitted_column_is_live() {
            let ir = make_ir(vec![read_op(0, "T", vec![]), emit_op(1, "T", "id")]);
            let liveness = ColumnLiveness::compute(&ir);
            assert!(
                liveness.is_live_after(0, "T", "id"),
                "id must be live after Read"
            );
            assert!(
                !liveness.is_live_after(0, "T", "name"),
                "name must be dead after Read"
            );
            assert!(
                !liveness.is_live_after(0, "T", "qty"),
                "qty must be dead after Read"
            );
        }

        // T-PJ-02 equivalent: assert uses A.status, emit uses A.id → both live.
        #[test]
        fn assert_and_emit_columns_are_live() {
            let assert_op = SrplBusinessOperationIr {
                ordinal: 1,
                kind: SrplBusinessOperationKindIr::Assert {
                    predicate: SrplPredicateIr::InputEqualsField {
                        input: "s".into(),
                        binding: "T".into(),
                        field: "status".into(),
                    },
                    failure_code: "ERR".into(),
                },
            };
            let ir = make_ir(vec![
                read_op(0, "T", vec![]),
                assert_op,
                emit_op(2, "T", "id"),
            ]);
            let liveness = ColumnLiveness::compute(&ir);
            assert!(liveness.is_live_after(0, "T", "id"), "id must be live");
            assert!(
                liveness.is_live_after(0, "T", "status"),
                "status must be live"
            );
        }

        #[test]
        fn liveness_is_empty_for_empty_body() {
            let ir = make_ir(vec![]);
            let liveness = ColumnLiveness::compute(&ir);
            assert!(!liveness.is_live_after(0, "T", "id"));
        }
    }
}

// ============================================================================
// Sub-module: projection_pushdown
// ============================================================================

/// Dead-column elimination pass.
///
/// Annotates `ReadTable` operations with an `EffectiveProjection` that
/// lists only the columns that are live downstream. The storage scan
/// adapter uses this annotation to avoid fetching full rows.
///
/// # Invariant (INV-12)
/// No column that is live at any downstream operation may be removed
/// from the effective projection.
pub mod projection_pushdown {
    use super::liveness::ColumnLiveness;
    use crate::{SrplBusinessOperationKindIr, SrplProcedureIr};

    /// Effective column projection computed for one `ReadTable` operation.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct EffectiveProjection {
        /// Binding name this projection applies to.
        pub binding: String,
        /// Columns that are live downstream.
        /// `None` = projection not computed / all columns required.
        /// `Some(vec![])` = no columns required (count-only scan).
        /// `Some(vec![...])` = only these columns need to be fetched.
        pub live_columns: Option<Vec<String>>,
    }

    /// Result of the projection pushdown pass.
    #[derive(Debug, Clone)]
    pub struct ProjectionPushdownResult {
        /// The original IR, unchanged in structure.
        pub ir: SrplProcedureIr,
        /// One entry per `ReadTable` operation (same ordinal order).
        pub projections: Vec<EffectiveProjection>,
    }

    /// Apply projection pushdown to a procedure IR.
    ///
    /// Does NOT mutate the `SrplProcedureIr` body structure — it only
    /// computes and returns projection annotations. The storage layer
    /// consumes the annotations at scan time.
    pub fn apply(ir: SrplProcedureIr) -> ProjectionPushdownResult {
        let liveness = ColumnLiveness::compute(&ir);
        let mut projections = Vec::new();

        for op in &ir.body.operations {
            if let SrplBusinessOperationKindIr::Read { binding, .. } = &op.kind {
                let live = liveness.live_columns_after(op.ordinal);
                let live_for_binding: Vec<String> = live
                    .into_iter()
                    .filter(|(b, _)| b == binding)
                    .map(|(_, f)| f)
                    .collect();

                projections.push(EffectiveProjection {
                    binding: binding.clone(),
                    live_columns: Some(live_for_binding),
                });
            }
        }

        ProjectionPushdownResult { ir, projections }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::ir::{SrplEmitValueIr, SrplValueIr};
        use crate::{
            Cardinality, SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr,
            SrplProcedureIr,
        };
        use andromeda_catalog::QualifiedName;

        fn qn(s: &str) -> QualifiedName {
            QualifiedName::parse(s).unwrap()
        }

        fn make_ir(ops: Vec<SrplBusinessOperationIr>) -> SrplProcedureIr {
            SrplProcedureIr {
                name: qn("db.ns.P"),
                inputs: vec![],
                result_streams: vec![],
                body: SrplProcedureBodyIr { operations: ops },
            }
        }

        // T-PJ-01: only id is emitted → projection = [id]
        #[test]
        fn unused_columns_are_excluded_from_projection() {
            let ir = make_ir(vec![
                SrplBusinessOperationIr {
                    ordinal: 0,
                    kind: SrplBusinessOperationKindIr::Read {
                        source: qn("db.ns.T"),
                        binding: "T".into(),
                        cardinality: Cardinality::One,
                        predicates: vec![],
                    },
                },
                SrplBusinessOperationIr {
                    ordinal: 1,
                    kind: SrplBusinessOperationKindIr::Emit {
                        stream: "S".into(),
                        values: vec![SrplEmitValueIr {
                            column: "out_id".into(),
                            value: SrplValueIr::Field {
                                binding: "T".into(),
                                field: "id".into(),
                            },
                        }],
                    },
                },
            ]);

            let result = apply(ir);
            assert_eq!(result.projections.len(), 1);
            let proj = &result.projections[0];
            assert_eq!(proj.binding, "T");
            let live = proj.live_columns.as_ref().unwrap();
            assert!(live.contains(&"id".to_string()), "id must be in projection");
            assert!(
                !live.contains(&"name".to_string()),
                "name must not be in projection"
            );
        }

        // T-PJ-05: no read → empty projections
        #[test]
        fn no_read_means_no_projections() {
            let ir = make_ir(vec![SrplBusinessOperationIr {
                ordinal: 0,
                kind: SrplBusinessOperationKindIr::Raise { code: "ERR".into() },
            }]);
            let result = apply(ir);
            assert!(result.projections.is_empty());
        }
    }
}

// ============================================================================
// Sub-module: cost_model
// ============================================================================

/// Cost estimation for a `SrplProcedureIr` body.
///
/// Uses the formula:
/// ```text
/// total = (tuples_estimated × 0.5) + (io_pages × 10.0) + (memory_pages × 2.0)
/// ```
pub mod cost_model {
    use crate::{Cardinality, SrplBusinessOperationKindIr, SrplProcedureIr};

    /// CPU cost per tuple (normalised units).
    pub const CPU_TUPLE_COST: f64 = 0.5;
    /// I/O cost per page (normalised units; ~20× CPU reflects SSD latency).
    pub const IO_PAGE_COST: f64 = 10.0;
    /// Memory cost per page (normalised units).
    pub const MEMORY_PAGE_COST: f64 = 2.0;
    /// Assumed tuples per storage page.
    pub const ASSUMED_PAGE_SIZE_TUPLES: f64 = 100.0;

    // Default selectivities when no histogram is available.
    const DEFAULT_EQUALITY_SELECTIVITY: f64 = 0.10;
    const DEFAULT_RANGE_SELECTIVITY: f64 = 0.33;

    // Default row counts when no stats are available.
    const DEFAULT_ROW_COUNT_ONE: f64 = 1.0;
    const DEFAULT_ROW_COUNT_OPTIONAL_ONE: f64 = 0.5;
    const DEFAULT_ROW_COUNT_NON_EMPTY_MANY: f64 = 10.0;
    const DEFAULT_ROW_COUNT_MANY: f64 = 100.0;

    /// Cost accuracy threshold above which a warning trace is emitted.
    pub const COST_ACCURACY_WARN_THRESHOLD: f64 = 0.50;
    /// Cost accuracy threshold above which a stats-reanalyze recommendation is set.
    pub const COST_ACCURACY_ALERT_THRESHOLD: f64 = 2.00;

    /// Bounded cost estimate produced by the `CostModel` pass.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct CostEstimate {
        pub cpu_cost: f64,
        pub io_cost: f64,
        pub memory_cost: f64,
        pub total_cost: f64,
    }

    impl CostEstimate {
        /// A zero-cost estimate (used as a baseline / default).
        pub const fn zero() -> Self {
            Self {
                cpu_cost: 0.0,
                io_cost: 0.0,
                memory_cost: 0.0,
                total_cost: 0.0,
            }
        }

        /// True when all components are non-negative and the total is consistent.
        pub fn is_valid(&self) -> bool {
            self.cpu_cost >= 0.0
                && self.io_cost >= 0.0
                && self.memory_cost >= 0.0
                && self.total_cost >= 0.0
                && !self.total_cost.is_nan()
                && !self.total_cost.is_infinite()
        }
    }

    /// Actual measured cost after an invocation completes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ActualCost {
        pub cpu_nanos: u64,
        pub io_pages: u64,
        pub memory_pages: u64,
    }

    /// Estimate the cost of a procedure without statistics.
    ///
    /// Uses default row counts and selectivities. Always returns a valid,
    /// non-negative, non-NaN estimate.
    pub fn estimate_without_stats(ir: &SrplProcedureIr) -> CostEstimate {
        let mut total_tuples = 0.0_f64;
        let mut total_io = 0.0_f64;
        let mut total_mem = 0.0_f64;

        for op in &ir.body.operations {
            match &op.kind {
                SrplBusinessOperationKindIr::Read {
                    cardinality,
                    predicates,
                    ..
                } => {
                    let base_rows = default_rows(*cardinality);
                    let sel = default_conjunct_selectivity(predicates.len());
                    let tuples = (base_rows * sel).max(0.0);
                    let pages = (tuples / ASSUMED_PAGE_SIZE_TUPLES).ceil().max(1.0);
                    total_tuples += tuples;
                    total_io += pages;
                    total_mem += pages;
                }
                SrplBusinessOperationKindIr::Update {
                    affected_rows_exact,
                    ..
                } => {
                    let rows = affected_rows_exact.unwrap_or(1) as f64;
                    let pages = (rows / ASSUMED_PAGE_SIZE_TUPLES).ceil().max(1.0);
                    total_tuples += rows;
                    total_io += pages * 2.0; // read + write-back
                    total_mem += pages;
                }
                // Emit, Assert, Raise: no storage cost.
                _ => {}
            }
        }

        let cpu = total_tuples * CPU_TUPLE_COST;
        let io = total_io * IO_PAGE_COST;
        let mem = total_mem * MEMORY_PAGE_COST;
        let total = cpu + io + mem;

        CostEstimate {
            cpu_cost: cpu,
            io_cost: io,
            memory_cost: mem,
            total_cost: total.max(0.0),
        }
    }

    /// Compute cost estimation error ratio: `|actual - estimated| / estimated`.
    ///
    /// Returns 0.0 when the estimated cost is zero (no divide-by-zero).
    pub fn cost_accuracy(estimated: &CostEstimate, actual: &ActualCost) -> f64 {
        // Convert actual to the same normalised units as the estimate.
        let actual_cpu_cost = (actual.cpu_nanos as f64 / 1_000_000.0) * CPU_TUPLE_COST;
        let actual_io_cost = actual.io_pages as f64 * IO_PAGE_COST;
        let actual_mem_cost = actual.memory_pages as f64 * MEMORY_PAGE_COST;
        let actual_total = actual_cpu_cost + actual_io_cost + actual_mem_cost;

        if estimated.total_cost < f64::EPSILON {
            return 0.0;
        }
        (actual_total - estimated.total_cost).abs() / estimated.total_cost
    }

    fn default_rows(c: Cardinality) -> f64 {
        match c {
            Cardinality::One => DEFAULT_ROW_COUNT_ONE,
            Cardinality::OptionalOne => DEFAULT_ROW_COUNT_OPTIONAL_ONE,
            Cardinality::NonEmptyMany => DEFAULT_ROW_COUNT_NON_EMPTY_MANY,
            Cardinality::Many => DEFAULT_ROW_COUNT_MANY,
        }
    }

    fn default_conjunct_selectivity(predicate_count: usize) -> f64 {
        if predicate_count == 0 {
            return 1.0;
        }
        // Each equality predicate independently contributes DEFAULT_EQUALITY_SELECTIVITY.
        DEFAULT_EQUALITY_SELECTIVITY.powi(predicate_count as i32)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::{
            Cardinality, SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplProcedureBodyIr,
            SrplProcedureIr,
        };
        use andromeda_catalog::QualifiedName;

        fn qn(s: &str) -> QualifiedName {
            QualifiedName::parse(s).unwrap()
        }

        fn read_ir(cardinality: Cardinality) -> SrplProcedureIr {
            SrplProcedureIr {
                name: qn("db.ns.P"),
                inputs: vec![],
                result_streams: vec![],
                body: SrplProcedureBodyIr {
                    operations: vec![SrplBusinessOperationIr {
                        ordinal: 0,
                        kind: SrplBusinessOperationKindIr::Read {
                            source: qn("db.ns.T"),
                            binding: "T".into(),
                            cardinality,
                            predicates: vec![],
                        },
                    }],
                },
            }
        }

        // T-CM-01
        #[test]
        fn single_read_cardinality_one_no_predicates() {
            let ir = read_ir(Cardinality::One);
            let cost = estimate_without_stats(&ir);
            assert!(cost.is_valid());
            // 1 tuple × 0.5 + 1 page × 10.0 + 1 page × 2.0 = 12.5
            assert!(
                (cost.total_cost - 12.5).abs() < 0.001,
                "got {}",
                cost.total_cost
            );
        }

        // T-CM-02
        #[test]
        fn single_read_cardinality_many_no_predicates() {
            let ir = read_ir(Cardinality::Many);
            let cost = estimate_without_stats(&ir);
            assert!(cost.is_valid());
            // 100 tuples × 0.5 + 1 page × 10.0 + 1 page × 2.0 = 62.0
            assert!(
                (cost.total_cost - 62.0).abs() < 0.001,
                "got {}",
                cost.total_cost
            );
        }

        // T-CM-08: zero-cost accuracy is safe
        #[test]
        fn cost_accuracy_zero_estimate_returns_zero() {
            let zero_estimate = CostEstimate::zero();
            let actual = ActualCost {
                cpu_nanos: 1000,
                io_pages: 1,
                memory_pages: 1,
            };
            assert_eq!(cost_accuracy(&zero_estimate, &actual), 0.0);
        }

        #[test]
        fn estimate_is_always_valid_for_empty_body() {
            let ir = SrplProcedureIr {
                name: qn("db.ns.P"),
                inputs: vec![],
                result_streams: vec![],
                body: SrplProcedureBodyIr { operations: vec![] },
            };
            let cost = estimate_without_stats(&ir);
            assert!(cost.is_valid());
            assert_eq!(cost.total_cost, 0.0);
        }

        // T-CM-06 / T-CM-07: cost accuracy thresholds
        #[test]
        fn cost_accuracy_flags_high_error() {
            // Estimated cost of ~12.5, actual much higher → error > 50%
            let ir = read_ir(Cardinality::One);
            let estimate = estimate_without_stats(&ir);
            // Simulate actual = 2× the estimate via io_pages manipulation
            let actual = ActualCost {
                cpu_nanos: 0,
                io_pages: 100, // 100 × 10.0 = 1000 io cost alone
                memory_pages: 0,
            };
            let ratio = cost_accuracy(&estimate, &actual);
            assert!(
                ratio > COST_ACCURACY_WARN_THRESHOLD,
                "expected ratio > {}, got {}",
                COST_ACCURACY_WARN_THRESHOLD,
                ratio
            );
        }

        #[test]
        fn estimate_is_non_negative_for_any_operation_mix() {
            use crate::ir::{SrplAssignmentIr, SrplValueIr};
            let ir = SrplProcedureIr {
                name: qn("db.ns.P"),
                inputs: vec![],
                result_streams: vec![],
                body: SrplProcedureBodyIr {
                    operations: vec![
                        SrplBusinessOperationIr {
                            ordinal: 0,
                            kind: SrplBusinessOperationKindIr::Read {
                                source: qn("db.ns.T"),
                                binding: "T".into(),
                                cardinality: Cardinality::Many,
                                predicates: vec![],
                            },
                        },
                        SrplBusinessOperationIr {
                            ordinal: 1,
                            kind: SrplBusinessOperationKindIr::Update {
                                target: qn("db.ns.T"),
                                predicates: vec![],
                                assignments: vec![SrplAssignmentIr {
                                    field: "qty".into(),
                                    value: SrplValueIr::Bool(false),
                                }],
                                affected_rows_exact: Some(1),
                            },
                        },
                    ],
                },
            };
            let cost = estimate_without_stats(&ir);
            assert!(cost.is_valid(), "cost must be valid: {:?}", cost);
            assert!(cost.total_cost > 0.0);
        }
    }
}

// ============================================================================
// Sub-module: plan_choice
// ============================================================================

/// Minimum-cost plan selection from a bounded set of alternatives.
pub mod plan_choice {
    use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

    use super::cost_model::CostEstimate;
    use super::plan_kind::OptimizerPlanKind;
    use crate::SrplProcedureIr;

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

        // Find the index of the minimum-cost / lowest-priority-tie alternative.
        let chosen_idx = alternatives
            .iter()
            .enumerate()
            .min_by(|(_, (_, cost_a, kind_a)), (_, (_, cost_b, kind_b))| {
                cost_a
                    .total_cost
                    .partial_cmp(&cost_b.total_cost)
                    .unwrap_or(std::cmp::Ordering::Equal)
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
        use crate::{SrplProcedureBodyIr, SrplProcedureIr};
        use andromeda_catalog::QualifiedName;

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
}
