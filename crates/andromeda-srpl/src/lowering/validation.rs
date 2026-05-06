//! Shared validation helpers for the SRPL IR lowering pipeline.
//!
//! These functions are called across the pipeline and plan-binding stages.
//! They hold no state; all errors are returned as [`AndromedaResult`] or
//! [`SrplDiagnostic`] values.

use std::collections::{BTreeMap, BTreeSet};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ColumnDescriptor};

use crate::{
    ProcedureAst, SrplBusinessOperationKindIr, SrplPredicateIr, SrplProcedureBodyIr,
    SrplProcedureIr, SrplValueIr,
};

/// Verifies that every `Assert` and `Raise` operation in `body` uses a code
/// that appears in `declared_error_codes`.
pub(super) fn validate_declared_error_codes(
    body: &SrplProcedureBodyIr,
    declared_error_codes: &[String],
) -> AndromedaResult<()> {
    let declared = declared_error_codes
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for operation in &body.operations {
        let failure_code = match &operation.kind {
            SrplBusinessOperationKindIr::Assert { failure_code, .. } => failure_code,
            SrplBusinessOperationKindIr::Raise { code } => code,
            _ => continue,
        };

        if !declared.contains(failure_code.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL body error code is not declared by the procedure error policy",
            ));
        }
    }

    Ok(())
}

/// Validates that parameter names, result stream names, and column names inside
/// each result stream are all unique. Returns a [`crate::SrplDiagnostic`] on
/// the first duplicate found.
pub(super) fn validate_ast_names_for_diagnostics(
    ast: &ProcedureAst,
) -> Result<(), crate::SrplDiagnostic> {
    let mut parameter_names = BTreeSet::new();
    for parameter in &ast.parameters {
        if !parameter_names.insert(parameter.name.value.as_str()) {
            return Err(crate::SrplDiagnostic::new(
                crate::DiagnosticPhase::Binding,
                Some(parameter.name.span),
                "SRPL procedure input names must be unique",
            ));
        }
    }

    let mut result_names = BTreeSet::new();
    for result in &ast.results {
        if !result_names.insert(result.name.value.as_str()) {
            return Err(crate::SrplDiagnostic::new(
                crate::DiagnosticPhase::Binding,
                Some(result.name.span),
                "SRPL result stream names must be unique",
            ));
        }

        let mut column_names = BTreeSet::new();
        for column in &result.columns {
            if !column_names.insert(column.name.value.as_str()) {
                return Err(crate::SrplDiagnostic::new(
                    crate::DiagnosticPhase::Binding,
                    Some(column.name.span),
                    "SRPL result column names must be unique",
                ));
            }
        }
    }

    Ok(())
}

/// Validates every predicate in the slice.
pub(super) fn validate_predicates(
    predicates: &[SrplPredicateIr],
    inputs: &[ColumnDescriptor],
    binding_sources: &BTreeMap<String, Vec<ColumnDescriptor>>,
) -> AndromedaResult<()> {
    for predicate in predicates {
        validate_predicate(predicate, inputs, binding_sources)?;
    }
    Ok(())
}

/// Validates a single predicate against the known inputs and bound read bindings.
pub(super) fn validate_predicate(
    predicate: &SrplPredicateIr,
    inputs: &[ColumnDescriptor],
    binding_sources: &BTreeMap<String, Vec<ColumnDescriptor>>,
) -> AndromedaResult<()> {
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
            require_input(inputs, input)?;
            let columns = binding_sources.get(binding).ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    "SRPL predicate references an unbound read binding",
                )
            })?;
            require_column(columns, field, "SRPL predicate field")?;
        }
    }
    Ok(())
}

/// Validates an IR value expression against the known inputs and bound read bindings.
pub(super) fn validate_value(
    value: &SrplValueIr,
    inputs: &[ColumnDescriptor],
    binding_sources: &BTreeMap<String, Vec<ColumnDescriptor>>,
) -> AndromedaResult<()> {
    match value {
        SrplValueIr::Input(input) => require_input(inputs, input)?,
        SrplValueIr::Field { binding, field }
        | SrplValueIr::SubtractInput { binding, field, .. } => {
            let columns = binding_sources.get(binding).ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    "SRPL value references an unbound read binding",
                )
            })?;
            require_column(columns, field, "SRPL value field")?;
            if let SrplValueIr::SubtractInput { input, .. } = value {
                require_input(inputs, input)?;
            }
        }
        SrplValueIr::Bool(_) => {}
        SrplValueIr::Constant(literal) => literal.validate()?,
        SrplValueIr::BinaryArith { left, right, .. } => {
            validate_value(left, inputs, binding_sources)?;
            validate_value(right, inputs, binding_sources)?;
        }
    }
    Ok(())
}

/// Asserts that `name` appears in `inputs`.
pub(super) fn require_input(inputs: &[ColumnDescriptor], name: &str) -> AndromedaResult<()> {
    if inputs.iter().any(|input| input.name == name) {
        Ok(())
    } else {
        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "SRPL body references an unknown procedure input",
        ))
    }
}

/// Asserts that `name` appears in `columns`.
pub(super) fn require_column(
    columns: &[ColumnDescriptor],
    name: &str,
    context: &str,
) -> AndromedaResult<()> {
    if columns.iter().any(|column| column.name == name) {
        Ok(())
    } else {
        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} references an unknown field"),
        ))
    }
}

/// Walks the entire IR body and rejects any symbol that resembles an SQL
/// keyword, enforcing the no-ad-hoc-SQL doctrine at the IR layer.
pub(super) fn validate_no_sql_like_symbols(ir: &SrplProcedureIr) -> AndromedaResult<()> {
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
