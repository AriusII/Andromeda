use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{CatalogObjectRef, ObjectKind};
use andromeda_srpl_ir::{BoundSrplOperationPlan, ExecutableProcedurePlan};

use crate::identifier::validate_srpl_identifier as validate_symbol;

use super::expression::{
    validate_assignment, validate_bounded_cardinality, validate_emit_value, validate_predicate,
    validate_predicates,
};

/// Validates the whole plan before adapter side effects are possible.
///
/// The present bound IR enum has no unsupported variants: every current variant
/// is matched by this validator and by the operation executor. Future variants
/// will require an explicit compiler change because Rust's exhaustive matching
/// will fail compilation.
pub(super) fn validate_plan(plan: &ExecutableProcedurePlan) -> AndromedaResult<()> {
    plan.validate()?;

    for operation in &plan.body.operations {
        match operation {
            BoundSrplOperationPlan::ReadTable {
                source,
                binding,
                cardinality,
                predicates,
                ..
            } => {
                source.validate_for_definition(ObjectKind::Table)?;
                require_evidence_for_table(plan, source)?;
                validate_symbol(binding, "SRPL read binding")?;
                validate_bounded_cardinality(*cardinality, "SRPL read")?;
                validate_predicates(predicates)?;
            },
            BoundSrplOperationPlan::Assert {
                predicate,
                failure_code,
                ..
            } => {
                validate_predicate(predicate)?;
                validate_symbol(failure_code, "SRPL assertion failure code")?;
            },
            BoundSrplOperationPlan::UpdateTable {
                target,
                predicates,
                assignments,
                affected_rows_exact,
                ..
            } => {
                target.validate_for_definition(ObjectKind::Table)?;
                require_evidence_for_table(plan, target)?;
                validate_predicates(predicates)?;
                if assignments.is_empty() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL interpreter update operation must declare assignments",
                    ));
                }
                for assignment in assignments {
                    validate_assignment(assignment)?;
                }
                match affected_rows_exact {
                    Some(rows) if *rows > 0 => {},
                    Some(_) => {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Srpl,
                            "SRPL interpreter update exact affected-row contract must be greater than zero",
                        ));
                    },
                    None => {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Srpl,
                            "SRPL interpreter rejects unbounded update affected-row contracts",
                        ));
                    },
                }
            },
            BoundSrplOperationPlan::Emit { stream, values, .. } => {
                validate_symbol(stream, "SRPL emit stream")?;
                if values.is_empty() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL interpreter emit operation must declare values",
                    ));
                }
                for value in values {
                    validate_emit_value(value)?;
                }
            },
            BoundSrplOperationPlan::Raise { code, .. } => {
                validate_symbol(code, "SRPL raise code")?;
            },
        }
    }

    Ok(())
}

fn require_evidence_for_table(
    plan: &ExecutableProcedurePlan,
    object: &CatalogObjectRef,
) -> AndromedaResult<()> {
    let evidence = plan
        .evidence
        .bound_objects
        .iter()
        .find(|bound| bound.object == *object && bound.kind == ObjectKind::Table)
        .ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL interpreter table operation lacks matching binding evidence",
            )
        })?;
    evidence.validate(ObjectKind::Table)?;
    if evidence.object.catalog_version != plan.evidence.catalog_version {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL interpreter table evidence catalog version mismatch",
        ));
    }
    Ok(())
}
