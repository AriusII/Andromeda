//! Optimizer regression suite for SRPL optimizer passes.

#![forbid(unsafe_code)]

use proptest::prelude::*;

use andromeda_catalog::{PlanClass, QualifiedName};
use andromeda_core::ScalarType;
use andromeda_srpl::{
    Cardinality, SrplAssignmentIr, SrplBusinessOperationIr, SrplBusinessOperationKindIr,
    SrplEmitValueIr, SrplPredicateIr, SrplProcedureBodyIr, SrplProcedureIr, SrplValueIr,
    optimizer::{
        constant_fold::{fold_assignments, fold_emit_values, fold_value},
        cost_model::{
            ActualCost, COST_ACCURACY_ALERT_THRESHOLD, COST_ACCURACY_WARN_THRESHOLD, CostEstimate,
            cost_accuracy, estimate_without_stats,
        },
        function_fold::{FunctionDeterminism, classify_builtin},
        liveness::ColumnLiveness,
        normalize::normalize,
        phase::OptimizerPhase,
        plan_choice::{RejectionReason, choose},
        plan_kind::OptimizerPlanKind,
        predicate_fold::{SimplifiedPredicates, simplify_predicates},
        predicate_pushdown::apply as pushdown_apply,
        projection_pushdown::apply as proj_apply,
    },
    procedure_model::{ArithOp, ConstantLiteral, MAX_EXPR_DEPTH, MAX_SRPL_BODY_OPERATIONS},
};

fn qn(s: &str) -> QualifiedName {
    QualifiedName::parse(s).unwrap()
}

fn int(n: i64) -> SrplValueIr {
    SrplValueIr::Constant(ConstantLiteral::Int64(n))
}

fn uint(n: u64) -> SrplValueIr {
    SrplValueIr::Constant(ConstantLiteral::Uint64(n))
}

fn bool_const(b: bool) -> SrplValueIr {
    SrplValueIr::Constant(ConstantLiteral::Bool(b))
}

fn arith(op: ArithOp, l: SrplValueIr, r: SrplValueIr) -> SrplValueIr {
    SrplValueIr::BinaryArith {
        op,
        left: Box::new(l),
        right: Box::new(r),
    }
}

fn eq_pred(input: &str, binding: &str, field: &str) -> SrplPredicateIr {
    SrplPredicateIr::InputEqualsField {
        input: input.into(),
        binding: binding.into(),
        field: field.into(),
    }
}

fn gte_pred(binding: &str, field: &str, input: &str) -> SrplPredicateIr {
    SrplPredicateIr::FieldGreaterThanOrEqualInput {
        binding: binding.into(),
        field: field.into(),
        input: input.into(),
    }
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

fn read_op_many(ordinal: u32, binding: &str) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
        kind: SrplBusinessOperationKindIr::Read {
            source: qn("db.ns.T"),
            binding: binding.into(),
            cardinality: Cardinality::Many,
            predicates: vec![],
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

fn update_op(
    ordinal: u32,
    target: &str,
    predicates: Vec<SrplPredicateIr>,
    assignments: Vec<SrplAssignmentIr>,
) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
        kind: SrplBusinessOperationKindIr::Update {
            target: qn(target),
            predicates,
            assignments,
            affected_rows_exact: None,
        },
    }
}

fn emit_op(ordinal: u32, binding: &str, field: &str) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
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

fn raise_op(ordinal: u32) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
        kind: SrplBusinessOperationKindIr::Raise {
            code: "E001".into(),
        },
    }
}

fn simple_assignment(field: &str, value: SrplValueIr) -> SrplAssignmentIr {
    SrplAssignmentIr {
        field: field.into(),
        value,
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

fn cost(total: f64) -> CostEstimate {
    CostEstimate {
        cpu_cost: total * 0.5,
        io_cost: total * 0.4,
        memory_cost: total * 0.1,
        total_cost: total,
    }
}

fn empty_ir() -> SrplProcedureIr {
    make_ir(vec![])
}

include!("optimizer/constant_folding.rs");
include!("optimizer/predicate_normalize.rs");
include!("optimizer/projection_pushdown.rs");
include!("optimizer/cost_plan_function_phase.rs");
include!("optimizer/property_fuzz.rs");
include!("optimizer/crash_determinism.rs");
include!("optimizer/integration_performance.rs");
