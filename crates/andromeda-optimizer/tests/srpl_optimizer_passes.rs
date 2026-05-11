//! Optimizer regression suite for SRPL optimizer passes.

#![forbid(unsafe_code)]

use andromeda_optimizer::srpl::{
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
};
use andromeda_plan_cache::PlanClass;
use andromeda_procedure_contract::QualifiedName;
use andromeda_srpl_ir::{
    ArithOp, Cardinality, ConstantLiteral, MAX_EXPR_DEPTH, MAX_SRPL_BODY_OPERATIONS,
    SrplAssignmentIr, SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplEmitValueIr,
    SrplPredicateIr, SrplProcedureBodyIr, SrplProcedureIr, SrplValueIr,
};
use andromeda_types::ScalarType;

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
    read_op_with_cardinality(ordinal, binding, Cardinality::One, predicates)
}

fn read_op_with_cardinality(
    ordinal: u32,
    binding: &str,
    cardinality: Cardinality,
    predicates: Vec<SrplPredicateIr>,
) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
        kind: SrplBusinessOperationKindIr::Read {
            source: qn("db.ns.T"),
            binding: binding.into(),
            cardinality,
            predicates,
        },
    }
}

fn read_op_many(ordinal: u32, binding: &str) -> SrplBusinessOperationIr {
    read_op_with_cardinality(ordinal, binding, Cardinality::Many, vec![])
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
    emit_values_op(ordinal, vec![emit_field_value("out", binding, field)])
}

fn emit_values_op(ordinal: u32, values: Vec<SrplEmitValueIr>) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
        kind: SrplBusinessOperationKindIr::Emit {
            stream: "S".into(),
            values,
        },
    }
}

fn emit_field_value(column: &str, binding: &str, field: &str) -> SrplEmitValueIr {
    SrplEmitValueIr {
        column: column.into(),
        value: SrplValueIr::Field {
            binding: binding.into(),
            field: field.into(),
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
        cpu_cost: total * 0.40,
        logical_io_cost: total * 0.15,
        physical_io_cost: total * 0.15,
        wal_cost: total * 0.10,
        temp_cost: total * 0.10,
        network_cost: total * 0.05,
        risk_penalty_cost: total * 0.05,
        total_cost: total,
    }
}

fn simplified_predicates(predicates: Vec<SrplPredicateIr>) -> Vec<SrplPredicateIr> {
    match simplify_predicates(predicates) {
        SimplifiedPredicates::Predicates(predicates) => predicates,
        SimplifiedPredicates::AlwaysFalse => panic!("unexpected AlwaysFalse"),
    }
}

fn empty_ir() -> SrplProcedureIr {
    make_ir(vec![])
}

#[path = "srpl_optimizer/constant_folding.rs"]
mod constant_folding;
#[path = "srpl_optimizer/cost_plan_function_contract.rs"]
mod cost_plan_function_contract;
#[path = "srpl_optimizer/crash_determinism.rs"]
mod crash_determinism;
#[path = "srpl_optimizer/integration_performance.rs"]
mod integration_performance;
#[path = "srpl_optimizer/predicate_normalize.rs"]
mod predicate_normalize;
#[path = "srpl_optimizer/projection_pushdown.rs"]
mod projection_pushdown;
#[path = "srpl_optimizer/property_fuzz.rs"]
mod property_fuzz;
