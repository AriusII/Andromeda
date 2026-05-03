use andromeda_catalog::{
    CatalogObjectRef, ObjectKind, ProcedureContractCandidate, ResultStreamContract,
};
use andromeda_core::AndromedaResult;
use std::collections::BTreeSet;

use crate::{
    BoundProcedure, BusinessOperationKindAst, ProcedureAst, ProcedureBodyAst, SrplAssignmentIr,
    SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplDiagnostic, SrplEmitValueIr,
    SrplPredicateIr, SrplProcedureBodyIr, SrplProcedureContractMetadata, SrplProcedureIr,
    SrplResultStreamIr, SrplValueIr,
};

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

pub fn lower_body_ast(body: ProcedureBodyAst) -> AndromedaResult<SrplProcedureBodyIr> {
    let operations = body
        .operations
        .into_iter()
        .map(|operation| SrplBusinessOperationIr {
            ordinal: operation.ordinal,
            kind: match operation.kind {
                BusinessOperationKindAst::Read {
                    source,
                    binding,
                    cardinality,
                } => SrplBusinessOperationKindIr::Read {
                    source: source.value,
                    binding: binding.value,
                    cardinality: cardinality.value,
                    predicates: Vec::new(),
                },
                BusinessOperationKindAst::Assert {
                    predicate,
                    failure_code,
                } => SrplBusinessOperationKindIr::Assert {
                    predicate: SrplPredicateIr::InputEqualsField {
                        input: predicate.value,
                        binding: "scope".to_string(),
                        field: "value".to_string(),
                    },
                    failure_code: failure_code.value,
                },
                BusinessOperationKindAst::Update { target, mutation } => {
                    SrplBusinessOperationKindIr::Update {
                        target: target.value,
                        predicates: Vec::new(),
                        assignments: vec![SrplAssignmentIr {
                            field: mutation.value,
                            value: SrplValueIr::Input("value".to_string()),
                        }],
                    }
                }
                BusinessOperationKindAst::Emit { stream, values } => {
                    SrplBusinessOperationKindIr::Emit {
                        stream: stream.value,
                        values: values
                            .into_iter()
                            .map(|value| SrplEmitValueIr {
                                column: value.value,
                                value: SrplValueIr::Bool(true),
                            })
                            .collect(),
                    }
                }
                BusinessOperationKindAst::Raise { code } => {
                    SrplBusinessOperationKindIr::Raise { code: code.value }
                }
            },
        })
        .collect();
    let ir = SrplProcedureBodyIr { operations };
    ir.validate_bounded()?;
    Ok(ir)
}

pub fn compile_narrow_procedure_signature(
    source: &str,
) -> Result<SrplProcedureIr, crate::SrplDiagnostic> {
    let srpl_source = crate::SrplSource::new(source);
    if let Some(diagnostic) = srpl_source
        .forbidden_construct_diagnostics()
        .into_iter()
        .next()
    {
        return Err(diagnostic);
    }

    let ast = crate::parse_procedure_signature(source)?;
    validate_ast_names_for_diagnostics(&ast)?;
    let bound = crate::bind_procedure(ast).map_err(|error| {
        crate::SrplDiagnostic::new(crate::DiagnosticPhase::Binding, None, error.to_string())
    })?;
    lower_bound_procedure(bound).map_err(|error| {
        crate::SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
    })
}

pub fn lower_ir_to_contract_candidate(
    ir: SrplProcedureIr,
    metadata: SrplProcedureContractMetadata,
) -> AndromedaResult<ProcedureContractCandidate> {
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

    Ok(ProcedureContractCandidate {
        object: CatalogObjectRef {
            object_id: metadata.object_id,
            name: ir.name,
            kind: ObjectKind::Procedure,
            catalog_version: metadata.catalog_version,
        },
        procedure_id: metadata.procedure_id,
        inputs: ir.inputs,
        structured_inputs: metadata.structured_inputs,
        result_streams: ir
            .result_streams
            .into_iter()
            .map(|result| ResultStreamContract {
                name: result.name,
                columns: result.columns,
                row_count_exact_required: result.cardinality.requires_exact_row_count(),
            })
            .collect(),
        required_permissions: metadata.required_permissions,
        transaction_policy: metadata.transaction_policy,
        compatibility_policy: metadata.compatibility_policy,
    })
}

pub fn compile_narrow_procedure_contract_candidate(
    source: &str,
    metadata: SrplProcedureContractMetadata,
) -> Result<ProcedureContractCandidate, crate::SrplDiagnostic> {
    let ir = compile_narrow_procedure_signature(source)?;
    lower_ir_to_contract_candidate(ir, metadata).map_err(|error| {
        crate::SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
    })
}

pub fn inventory_reserve_stock_body_ir() -> Result<SrplProcedureBodyIr, SrplDiagnostic> {
    let body = SrplProcedureBodyIr {
        operations: vec![
            SrplBusinessOperationIr {
                ordinal: 0,
                kind: SrplBusinessOperationKindIr::Read {
                    source: andromeda_catalog::QualifiedName::parse("Inventory.ProductStock")
                        .map_err(|error| {
                            SrplDiagnostic::new(
                                crate::DiagnosticPhase::IrLowering,
                                None,
                                error.to_string(),
                            )
                        })?,
                    binding: "Stock".to_string(),
                    cardinality: crate::Cardinality::One,
                    predicates: vec![SrplPredicateIr::InputEqualsField {
                        input: "ProductId".to_string(),
                        binding: "Stock".to_string(),
                        field: "ProductId".to_string(),
                    }],
                },
            },
            SrplBusinessOperationIr {
                ordinal: 1,
                kind: SrplBusinessOperationKindIr::Assert {
                    predicate: SrplPredicateIr::FieldGreaterThanOrEqualInput {
                        binding: "Stock".to_string(),
                        field: "AvailableQuantity".to_string(),
                        input: "Quantity".to_string(),
                    },
                    failure_code: "InsufficientStock".to_string(),
                },
            },
            SrplBusinessOperationIr {
                ordinal: 2,
                kind: SrplBusinessOperationKindIr::Update {
                    target: andromeda_catalog::QualifiedName::parse("Inventory.ProductStock")
                        .map_err(|error| {
                            SrplDiagnostic::new(
                                crate::DiagnosticPhase::IrLowering,
                                None,
                                error.to_string(),
                            )
                        })?,
                    predicates: vec![SrplPredicateIr::InputEqualsField {
                        input: "ProductId".to_string(),
                        binding: "Stock".to_string(),
                        field: "ProductId".to_string(),
                    }],
                    assignments: vec![SrplAssignmentIr {
                        field: "AvailableQuantity".to_string(),
                        value: SrplValueIr::SubtractInput {
                            binding: "Stock".to_string(),
                            field: "AvailableQuantity".to_string(),
                            input: "Quantity".to_string(),
                        },
                    }],
                },
            },
            SrplBusinessOperationIr {
                ordinal: 3,
                kind: SrplBusinessOperationKindIr::Emit {
                    stream: "Reservation".to_string(),
                    values: vec![SrplEmitValueIr {
                        column: "Reserved".to_string(),
                        value: SrplValueIr::Bool(true),
                    }],
                },
            },
        ],
    };
    body.validate_bounded().map_err(|error| {
        SrplDiagnostic::new(crate::DiagnosticPhase::IrLowering, None, error.to_string())
    })?;
    Ok(body)
}

fn validate_ast_names_for_diagnostics(ast: &ProcedureAst) -> Result<(), crate::SrplDiagnostic> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cardinality, DiagnosticPhase};

    #[test]
    fn compiles_narrow_signature_to_ir_without_execution_surface() {
        let ir = compile_narrow_procedure_signature(
            "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);",
        )
            .unwrap();

        assert_eq!(ir.name.as_catalog_path(), "Inventory.ReserveStock");
        assert_eq!(ir.inputs.len(), 1);
        assert_eq!(ir.result_streams[0].cardinality, Cardinality::One);
        assert!(ir.body.operations.is_empty());
    }

    #[test]
    fn compile_reports_forbidden_construct_before_lowering() {
        let diagnostic = compile_narrow_procedure_signature(
            "procedure X accepts () returns R many (C bool); execute sql",
        )
        .unwrap_err();

        assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
        assert!(diagnostic.location.is_some());
    }

    #[test]
    fn compile_reports_binding_duplicate_with_span() {
        let diagnostic = compile_narrow_procedure_signature(
            "procedure Inventory.ReserveStock accepts (ProductId i64, ProductId i64) returns Reservation one (Reserved bool);",
        )
            .unwrap_err();

        assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
        assert!(diagnostic.location.is_some());
        assert!(diagnostic.message.contains("unique"));
    }

    #[test]
    fn reserve_stock_body_skeleton_models_bounded_business_operations() {
        let body = inventory_reserve_stock_body_ir().unwrap();

        assert_eq!(body.operations.len(), 4);
        assert!(body.validate_bounded().is_ok());
        assert!(matches!(
            &body.operations[0].kind,
            SrplBusinessOperationKindIr::Read { .. }
        ));
        assert!(matches!(
            &body.operations[1].kind,
            SrplBusinessOperationKindIr::Assert { .. }
        ));
        assert!(matches!(
            &body.operations[2].kind,
            SrplBusinessOperationKindIr::Update { .. }
        ));
        assert!(matches!(
            &body.operations[3].kind,
            SrplBusinessOperationKindIr::Emit { .. }
        ));

        let SrplBusinessOperationKindIr::Update { assignments, .. } = &body.operations[2].kind
        else {
            unreachable!("operation 2 is checked as update");
        };
        assert_eq!(assignments[0].field, "AvailableQuantity");
        assert!(matches!(
            &assignments[0].value,
            SrplValueIr::SubtractInput { .. }
        ));
    }

    #[test]
    fn compiles_tiny_body_syntax_to_deterministic_ir_operations() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; assert Quantity InsufficientStock; update Inventory.ProductStock AvailableQuantity; emit Reservation (Reserved); }";
        let first = compile_narrow_procedure_signature(source).unwrap();
        let second = compile_narrow_procedure_signature(source).unwrap();

        assert_eq!(first.body, second.body);
        assert_eq!(first.body.operations.len(), 4);
        assert!(first.body.validate_bounded().is_ok());
        assert!(matches!(
            &first.body.operations[0].kind,
            SrplBusinessOperationKindIr::Read {
                binding,
                cardinality: Cardinality::One,
                ..
            } if binding.as_str() == "Stock"
        ));
        assert!(matches!(
            &first.body.operations[1].kind,
            SrplBusinessOperationKindIr::Assert {
                failure_code,
                ..
            } if failure_code.as_str() == "InsufficientStock"
        ));
        let SrplBusinessOperationKindIr::Update { assignments, .. } =
            &first.body.operations[2].kind
        else {
            panic!("operation 2 should lower to update");
        };
        assert_eq!(assignments[0].field, "AvailableQuantity");
        assert!(matches!(
            &first.body.operations[3].kind,
            SrplBusinessOperationKindIr::Emit { stream, values }
                if stream.as_str() == "Reservation"
                    && matches!(
                        values.first(),
                        Some(value) if value.column.as_str() == "Reserved"
                    )
        ));
    }

    #[test]
    fn body_validation_rejects_unbounded_or_sparse_operations() {
        let mut body = inventory_reserve_stock_body_ir().unwrap();
        body.operations[2].ordinal = 4;

        let error = body.validate_bounded().unwrap_err();

        assert!(error.message().contains("dense"));
    }
}
