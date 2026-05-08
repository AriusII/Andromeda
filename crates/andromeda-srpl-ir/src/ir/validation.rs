use andromeda_contract::QualifiedName;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::{
    procedure::{SrplBusinessOperationKindIr, SrplProcedureIr},
    values::{SrplPredicateIr, SrplValueIr},
};

pub(super) fn validate_qualified_name(name: &QualifiedName, context: &str) -> AndromedaResult<()> {
    if name.parts().is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} must not be empty"),
        ));
    }

    Ok(())
}

/// Walks the entire IR body and rejects any symbol that resembles an SQL
/// keyword, enforcing the no-ad-hoc-SQL doctrine at the IR layer.
pub fn validate_no_sql_like_symbols(ir: &SrplProcedureIr) -> AndromedaResult<()> {
    for operation in &ir.body.operations {
        match &operation.kind {
            SrplBusinessOperationKindIr::Read {
                binding,
                predicates,
                ..
            } => {
                reject_sql_like_symbol(binding)?;
                for predicate in predicates {
                    validate_predicate_symbols(predicate)?;
                }
            }
            SrplBusinessOperationKindIr::Assert {
                predicate,
                failure_code,
            } => {
                validate_predicate_symbols(predicate)?;
                reject_sql_like_symbol(failure_code)?;
            }
            SrplBusinessOperationKindIr::Update {
                predicates,
                assignments,
                affected_rows_exact: _,
                ..
            } => {
                for predicate in predicates {
                    validate_predicate_symbols(predicate)?;
                }
                for assignment in assignments {
                    reject_sql_like_symbol(&assignment.field)?;
                    validate_value_symbols(&assignment.value)?;
                }
            }
            SrplBusinessOperationKindIr::Emit { stream, values } => {
                reject_sql_like_symbol(stream)?;
                for value in values {
                    reject_sql_like_symbol(&value.column)?;
                    validate_value_symbols(&value.value)?;
                }
            }
            SrplBusinessOperationKindIr::Raise { code } => reject_sql_like_symbol(code)?,
        }
    }
    Ok(())
}

fn validate_predicate_symbols(predicate: &SrplPredicateIr) -> AndromedaResult<()> {
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
            reject_sql_like_symbol(input)?;
            reject_sql_like_symbol(binding)?;
            reject_sql_like_symbol(field)?;
        }
    }
    Ok(())
}

fn validate_value_symbols(value: &SrplValueIr) -> AndromedaResult<()> {
    match value {
        SrplValueIr::Input(input) => reject_sql_like_symbol(input)?,
        SrplValueIr::Field { binding, field } => {
            reject_sql_like_symbol(binding)?;
            reject_sql_like_symbol(field)?;
        }
        SrplValueIr::Bool(_) => {}
        SrplValueIr::SubtractInput {
            binding,
            field,
            input,
        } => {
            reject_sql_like_symbol(binding)?;
            reject_sql_like_symbol(field)?;
            reject_sql_like_symbol(input)?;
        }
        SrplValueIr::Constant(_) => {}
        SrplValueIr::BinaryArith { left, right, .. } => {
            validate_value_symbols(left)?;
            validate_value_symbols(right)?;
        }
    }
    Ok(())
}

fn reject_sql_like_symbol(symbol: &str) -> AndromedaResult<()> {
    let lower = symbol.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "select" | "update" | "insert" | "delete" | "merge" | "from" | "where" | "join" | "sql"
    ) {
        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "SRPL executable plan rejects SQL-like free-form symbols",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cardinality, SrplBusinessOperationIr, SrplProcedureBodyIr};

    #[test]
    fn ir_symbol_guard_rejects_sql_like_read_binding() {
        let ir = procedure_with_operation(SrplBusinessOperationKindIr::Read {
            source: QualifiedName::parse("Inventory.ProductStock").unwrap(),
            binding: "select".to_string(),
            cardinality: Cardinality::One,
            predicates: Vec::new(),
        });

        let error = validate_no_sql_like_symbols(&ir).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    }

    #[test]
    fn ir_symbol_guard_accepts_bounded_ir_symbols() {
        let ir = procedure_with_operation(SrplBusinessOperationKindIr::Read {
            source: QualifiedName::parse("Inventory.ProductStock").unwrap(),
            binding: "Stock".to_string(),
            cardinality: Cardinality::One,
            predicates: Vec::new(),
        });

        assert!(validate_no_sql_like_symbols(&ir).is_ok());
    }

    fn procedure_with_operation(kind: SrplBusinessOperationKindIr) -> SrplProcedureIr {
        SrplProcedureIr {
            name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            inputs: Vec::new(),
            result_streams: Vec::new(),
            body: SrplProcedureBodyIr {
                operations: vec![SrplBusinessOperationIr { ordinal: 0, kind }],
            },
        }
    }
}
