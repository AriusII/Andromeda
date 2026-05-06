//! High-level SRPL lowering pipeline: AST → IR → contract candidate → catalog definition.
//!
//! This module owns the public compile/lower entry points. Each function
//! corresponds to one stage of the compiler pipeline described in CLAUDE.md.

use andromeda_catalog::{
    CatalogDefinition, DefinitionBatch, DefinitionBatchId, DefinitionOperation,
    ProcedureContractCandidate, ResultStreamContract, inventory_reserve_stock_contract_candidate,
};
use andromeda_core::{AndromedaResult, CatalogVersion, DatabaseId, NamespaceId};

use crate::{
    BoundProcedure, BusinessOperationKindAst, ProcedureBodyAst, SrplAssignmentIr,
    SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplEmitValueIr, SrplPredicateIr,
    SrplProcedureBodyIr, SrplProcedureContractMetadata, SrplProcedureIr, SrplResultStreamIr,
    SrplValueIr,
    optimizer::{
        OptimizerPipelineConfig, OptimizerPipelineResult, optimize_procedure_ir_with_config,
    },
};

use super::validation::{validate_ast_names_for_diagnostics, validate_declared_error_codes};

/// Canonical PDF-style SRPL source for `Inventory.ReserveStock`.
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

fn enrich_source_diagnostic(
    source: &str,
    diagnostic: crate::SrplDiagnostic,
    procedure_name: Option<&str>,
) -> crate::SrplDiagnostic {
    let Some(span) = diagnostic.location else {
        return diagnostic;
    };
    let (line, column) = line_column(source, span.start);
    let mut message = diagnostic.message;
    if let Some(name) = procedure_name {
        message.push_str(&format!("; procedure {name}"));
    }
    message.push_str(&format!("; line {line}, column {column}"));
    crate::SrplDiagnostic::new(diagnostic.phase, Some(span), message)
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

/// Lowers a [`SrplProcedureIr`] and its [`SrplProcedureContractMetadata`] to a
/// [`ProcedureContractCandidate`] ready for catalog insertion.
pub fn lower_ir_to_contract_candidate(
    ir: SrplProcedureIr,
    metadata: SrplProcedureContractMetadata,
) -> AndromedaResult<ProcedureContractCandidate> {
    use andromeda_catalog::{CatalogObjectRef, ObjectKind};

    let signature = crate::ProcedureSignature {
        name: ir.name.clone(),
        accepts: ir.inputs.clone(),
        returns: ir
            .result_streams
            .iter()
            .map(|result| crate::ResultContract {
                name: result.name.clone(),
                cardinality: result.cardinality,
                columns: result.columns.clone(),
            })
            .collect(),
    };
    signature.validate()?;
    ir.body.validate_bounded()?;
    validate_declared_error_codes(&ir.body, &metadata.error_policy.allowed_error_codes)?;

    let candidate = ProcedureContractCandidate {
        object: CatalogObjectRef {
            object_id: metadata.object_id,
            name: ir.name,
            kind: ObjectKind::Procedure,
            catalog_version: metadata.catalog_version,
        },
        procedure_id: metadata.procedure_id,
        stats_version: metadata.stats_version,
        protocol_layout: metadata.protocol_layout,
        inputs: ir.inputs,
        structured_inputs: metadata.structured_inputs,
        result_streams: ir
            .result_streams
            .into_iter()
            .enumerate()
            .map(|(index, result)| ResultStreamContract {
                stream_id: (index as u64) + 1,
                name: result.name,
                columns: result.columns,
                row_count_exact_required: result.cardinality.requires_exact_row_count(),
            })
            .collect(),
        required_permissions: metadata.required_permissions,
        transaction_policy: metadata.transaction_policy,
        compatibility_policy: metadata.compatibility_policy,
        result_metadata_policy: metadata.result_metadata_policy,
        error_policy: metadata.error_policy,
        multi_result_policy: metadata.multi_result_policy,
    };
    candidate.clone().materialize()?;
    Ok(candidate)
}

/// Compiles a narrow SRPL source to a [`ProcedureContractCandidate`].
pub fn compile_narrow_procedure_contract_candidate(
    source: &str,
    metadata: SrplProcedureContractMetadata,
) -> Result<ProcedureContractCandidate, crate::SrplDiagnostic> {
    let ir = compile_narrow_procedure_signature(source)?;
    lower_ir_to_contract_candidate(ir, metadata).map_err(|error| {
        crate::SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
    })
}

/// Lowers a [`SrplProcedureIr`] to a [`CatalogDefinition`] ready for a
/// [`DefinitionBatch`].
pub fn lower_ir_to_catalog_definition(
    ir: SrplProcedureIr,
    metadata: SrplProcedureContractMetadata,
) -> AndromedaResult<CatalogDefinition> {
    let contract = lower_ir_to_contract_candidate(ir, metadata)?.materialize()?;
    Ok(CatalogDefinition::Procedure(contract))
}

/// Compiles a narrow SRPL source to a [`CatalogDefinition`].
pub fn compile_narrow_procedure_definition(
    source: &str,
    metadata: SrplProcedureContractMetadata,
) -> Result<CatalogDefinition, crate::SrplDiagnostic> {
    let ir = compile_narrow_procedure_signature(source)?;
    lower_ir_to_catalog_definition(ir, metadata).map_err(|error| {
        crate::SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
    })
}

/// Compiles a narrow SRPL source to a single-operation [`DefinitionBatch`].
pub fn compile_narrow_procedure_definition_batch(
    source: &str,
    metadata: SrplProcedureContractMetadata,
    batch_id: DefinitionBatchId,
    database_id: DatabaseId,
    namespace_id: NamespaceId,
    base_version: CatalogVersion,
) -> Result<DefinitionBatch, crate::SrplDiagnostic> {
    let definition = compile_narrow_procedure_definition(source, metadata)?;
    Ok(DefinitionBatch {
        batch_id,
        database_id,
        namespace_id,
        base_version,
        operations: vec![DefinitionOperation::Create(definition)],
    })
}

/// Builds the [`SrplProcedureContractMetadata`] for `Inventory.ReserveStock`
/// from the catalog fixture.
pub fn inventory_reserve_stock_contract_metadata(
    catalog_version: CatalogVersion,
) -> SrplProcedureContractMetadata {
    let fixture = inventory_reserve_stock_contract_candidate(catalog_version);
    SrplProcedureContractMetadata {
        object_id: fixture.object.object_id,
        procedure_id: fixture.procedure_id,
        catalog_version: fixture.object.catalog_version,
        stats_version: fixture.stats_version,
        protocol_layout: fixture.protocol_layout,
        structured_inputs: fixture.structured_inputs,
        required_permissions: fixture.required_permissions,
        transaction_policy: fixture.transaction_policy,
        compatibility_policy: fixture.compatibility_policy,
        result_metadata_policy: fixture.result_metadata_policy,
        error_policy: fixture.error_policy,
        multi_result_policy: fixture.multi_result_policy,
    }
}

/// Compiles the canonical `Inventory.ReserveStock` SRPL source to a
/// [`ProcedureContractCandidate`].
pub fn compile_inventory_reserve_stock_contract_candidate(
    catalog_version: CatalogVersion,
) -> Result<ProcedureContractCandidate, crate::SrplDiagnostic> {
    compile_narrow_procedure_contract_candidate(
        INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE,
        inventory_reserve_stock_contract_metadata(catalog_version),
    )
}

/// Compiles the canonical `Inventory.ReserveStock` SRPL source to a
/// materialized [`andromeda_catalog::ProcedureContract`].
pub fn compile_inventory_reserve_stock_contract(
    catalog_version: CatalogVersion,
) -> Result<andromeda_catalog::ProcedureContract, crate::SrplDiagnostic> {
    compile_inventory_reserve_stock_contract_candidate(catalog_version)?
        .materialize()
        .map_err(|error| {
            crate::SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
        })
}
