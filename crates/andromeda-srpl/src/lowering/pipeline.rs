//! High-level SRPL lowering pipeline: AST → IR.
//!
//! This module owns source parsing, binding, and typed IR lowering. Catalog
//! contract materialization lives in the sibling `contract` module.

use andromeda_error::AndromedaResult;

use crate::{
    BoundProcedure, BusinessOperationKindAst, ProcedureBodyAst, SrplAssignmentIr,
    SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplEmitValueIr, SrplPredicateIr,
    SrplProcedureBodyIr, SrplProcedureIr, SrplResultStreamIr, SrplValueIr,
    optimizer::{
        OptimizerPipelineConfig, OptimizerPipelineResult, optimize_procedure_ir_with_config,
    },
};

use super::{
    diagnostics::enrich_source_diagnostic, validation::validate_ast_names_for_diagnostics,
};

/// Canonical SRPL source for `Inventory.ReserveStock`.
pub const INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE: &str = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId and Stock.AvailableQuantity >= Quantity else fail InsufficientStock; update Inventory.ProductStock set AvailableQuantity = Stock.AvailableQuantity - Quantity where ProductId = Stock.ProductId affected rows 1; return Reservation (Reserved); end;";

/// Lowers a [`BoundProcedure`] to a [`SrplProcedureIr`].
pub fn lower_bound_procedure(bound: BoundProcedure) -> AndromedaResult<SrplProcedureIr> {
    bound.signature.validate()?;
    let body = lower_body_ast(bound.body)?;
    Ok(SrplProcedureIr {
        name: bound.signature.name,
        inputs: bound.signature.accepts,
        result_streams: bound
            .signature
            .returns
            .into_iter()
            .map(|result| SrplResultStreamIr {
                name: result.name,
                cardinality: result.cardinality,
                columns: result.columns,
            })
            .collect(),
        body,
    })
}

/// Lowers a [`ProcedureBodyAst`] to a [`SrplProcedureBodyIr`].
pub fn lower_body_ast(body: ProcedureBodyAst) -> AndromedaResult<SrplProcedureBodyIr> {
    let mut operations = Vec::new();
    for operation in body.operations {
        match operation.kind {
            BusinessOperationKindAst::Read {
                source,
                binding,
                cardinality,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Read {
                    source: source.value,
                    binding: binding.value,
                    cardinality: cardinality.value,
                    predicates: Vec::new(),
                },
            }),
            BusinessOperationKindAst::Assert {
                predicate,
                failure_code,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Assert {
                    predicate: SrplPredicateIr::InputEqualsField {
                        input: predicate.value,
                        binding: "scope".to_string(),
                        field: "value".to_string(),
                    },
                    failure_code: failure_code.value,
                },
            }),
            BusinessOperationKindAst::Update {
                target,
                mutation,
                affected_rows_exact,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Update {
                    target: target.value,
                    predicates: Vec::new(),
                    assignments: vec![SrplAssignmentIr {
                        field: mutation.value,
                        value: SrplValueIr::Input("value".to_string()),
                    }],
                    affected_rows_exact: affected_rows_exact.map(|rows| rows.value),
                },
            }),
            BusinessOperationKindAst::Emit { stream, values } => {
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Emit {
                        stream: stream.value,
                        values: values
                            .into_iter()
                            .map(|value| SrplEmitValueIr {
                                column: value.value,
                                value: SrplValueIr::Bool(true),
                            })
                            .collect(),
                    },
                })
            }
            BusinessOperationKindAst::Raise { code } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Raise { code: code.value },
            }),
            BusinessOperationKindAst::Ensure {
                source,
                binding,
                lookup_input,
                lookup_field,
                quantity_field,
                quantity_input,
                failure_code,
            } => {
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Read {
                        source: source.value,
                        binding: binding.value.clone(),
                        cardinality: crate::Cardinality::One,
                        predicates: vec![SrplPredicateIr::InputEqualsField {
                            input: lookup_input.value,
                            binding: binding.value.clone(),
                            field: lookup_field.value,
                        }],
                    },
                });
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Assert {
                        predicate: SrplPredicateIr::FieldGreaterThanOrEqualInput {
                            binding: binding.value,
                            field: quantity_field.value,
                            input: quantity_input.value,
                        },
                        failure_code: failure_code.value,
                    },
                });
            }
            BusinessOperationKindAst::UpdateSet {
                target,
                field,
                value_binding,
                value_field,
                value_input,
                where_input,
                where_binding,
                where_field,
                affected_rows_exact,
            } => operations.push(SrplBusinessOperationIr {
                ordinal: operations.len() as u32,
                kind: SrplBusinessOperationKindIr::Update {
                    target: target.value,
                    predicates: vec![SrplPredicateIr::InputEqualsField {
                        input: where_input.value,
                        binding: where_binding.value,
                        field: where_field.value,
                    }],
                    assignments: vec![SrplAssignmentIr {
                        field: field.value,
                        value: SrplValueIr::SubtractInput {
                            binding: value_binding.value,
                            field: value_field.value,
                            input: value_input.value,
                        },
                    }],
                    affected_rows_exact: Some(affected_rows_exact.value),
                },
            }),
            BusinessOperationKindAst::Return { stream, values } => {
                operations.push(SrplBusinessOperationIr {
                    ordinal: operations.len() as u32,
                    kind: SrplBusinessOperationKindIr::Emit {
                        stream: stream.value,
                        values: values
                            .into_iter()
                            .map(|value| SrplEmitValueIr {
                                column: value.value,
                                value: SrplValueIr::Bool(true),
                            })
                            .collect(),
                    },
                })
            }
        }
    }
    let ir = SrplProcedureBodyIr { operations };
    ir.validate_bounded()?;
    Ok(ir)
}

/// Parses, binds, and lowers a narrow SRPL procedure source to [`SrplProcedureIr`].
///
/// Returns a [`crate::SrplDiagnostic`] on the first detected violation.
pub fn compile_narrow_procedure_signature(
    source: &str,
) -> Result<SrplProcedureIr, crate::SrplDiagnostic> {
    let srpl_source = crate::SrplSource::new(source);
    if let Some(diagnostic) = srpl_source
        .forbidden_construct_diagnostics()
        .into_iter()
        .next()
    {
        return Err(enrich_source_diagnostic(source, diagnostic, None));
    }

    let ast = crate::parse_procedure_signature(source)
        .map_err(|diagnostic| enrich_source_diagnostic(source, diagnostic, None))?;
    let procedure_name = ast.name.value.as_catalog_path();
    validate_ast_names_for_diagnostics(&ast, source)?;
    let bound = crate::bind_procedure(ast).map_err(|error| {
        crate::SrplDiagnostic::new(
            crate::DiagnosticPhase::Binding,
            None,
            format!("{}; procedure {}", error, procedure_name),
        )
    })?;
    let procedure_name = bound.signature.name.as_catalog_path();
    lower_bound_procedure(bound).map_err(|error| {
        crate::SrplDiagnostic::new(
            crate::DiagnosticPhase::IrLowering,
            None,
            format!("{}; procedure {}", error, procedure_name),
        )
    })
}

/// Parses, binds, lowers, and optimizes a narrow SRPL procedure source.
///
/// Source diagnostics from parse/bind/lower phases are returned before the
/// optimizer is invoked, preserving their original source spans.
pub fn compile_narrow_procedure_signature_with_optimizer(
    source: &str,
    optimizer_config: OptimizerPipelineConfig,
) -> Result<OptimizerPipelineResult, crate::SrplDiagnostic> {
    let ir = compile_narrow_procedure_signature(source)?;
    let procedure_name = ir.name.as_catalog_path();
    let mut result = optimize_procedure_ir_with_config(ir, optimizer_config).map_err(|error| {
        crate::SrplDiagnostic::new(
            crate::DiagnosticPhase::IrLowering,
            None,
            format!("{}; procedure {}", error, procedure_name),
        )
    })?;
    let mut phases = vec![
        crate::optimizer::phase::OptimizerPhase::Parsing,
        crate::optimizer::phase::OptimizerPhase::Binding,
        crate::optimizer::phase::OptimizerPhase::IRLowering,
    ];
    phases.extend(std::mem::take(&mut result.phases));
    result.phases = phases;
    Ok(result)
}
