use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    Cardinality,
    execution_adapter::SrplRowBound,
    identifier::validate_srpl_identifier as validate_symbol,
    procedure_model::{SrplAssignmentIr, SrplEmitValueIr, SrplPredicateIr, SrplValueIr},
};

pub(super) fn row_bound_for_read(cardinality: Cardinality) -> AndromedaResult<SrplRowBound> {
    match cardinality {
        Cardinality::One => SrplRowBound::exact(1),
        Cardinality::OptionalOne => SrplRowBound::at_most(1),
        Cardinality::Many | Cardinality::NonEmptyMany => Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "SRPL interpreter rejects read operations without an intrinsic row bound",
        )),
    }
}

pub(super) fn validate_bounded_cardinality(
    cardinality: Cardinality,
    context: &str,
) -> AndromedaResult<()> {
    if cardinality.intrinsic_max_row_count().is_some() {
        Ok(())
    } else {
        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} operation must carry a bounded row contract"),
        ))
    }
}

pub(super) fn validate_predicates(predicates: &[SrplPredicateIr]) -> AndromedaResult<()> {
    for predicate in predicates {
        validate_predicate(predicate)?;
    }
    Ok(())
}

pub(super) fn validate_predicate(predicate: &SrplPredicateIr) -> AndromedaResult<()> {
    match predicate {
        SrplPredicateIr::InputEqualsField {
            input,
            binding,
            field,
        }
        | SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding,
            field,
            input,
        } => {
            validate_symbol(input, "SRPL predicate input")?;
            validate_symbol(binding, "SRPL predicate binding")?;
            validate_symbol(field, "SRPL predicate field")?;
        }
    }
    Ok(())
}

pub(super) fn validate_assignment(assignment: &SrplAssignmentIr) -> AndromedaResult<()> {
    validate_symbol(&assignment.field, "SRPL assignment field")?;
    validate_value(&assignment.value)
}

pub(super) fn validate_emit_value(value: &SrplEmitValueIr) -> AndromedaResult<()> {
    validate_symbol(&value.column, "SRPL emit column")?;
    validate_value(&value.value)
}

fn validate_value(value: &SrplValueIr) -> AndromedaResult<()> {
    match value {
        SrplValueIr::Input(input) => validate_symbol(input, "SRPL value input"),
        SrplValueIr::Field { binding, field } => {
            validate_symbol(binding, "SRPL value binding")?;
            validate_symbol(field, "SRPL value field")
        }
        SrplValueIr::Bool(_) => Ok(()),
        SrplValueIr::SubtractInput {
            binding,
            field,
            input,
        } => {
            validate_symbol(binding, "SRPL subtract binding")?;
            validate_symbol(field, "SRPL subtract field")?;
            validate_symbol(input, "SRPL subtract input")
        }
        SrplValueIr::Constant(literal) => literal.validate(),
        SrplValueIr::BinaryArith { left, right, .. } => {
            validate_value(left)?;
            validate_value(right)
        }
    }
}
