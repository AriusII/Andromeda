use andromeda_catalog::{
    CatalogObjectRef, ObjectKind, ProcedureContractCandidate, ResultStreamContract,
};
use andromeda_core::AndromedaResult;
use std::collections::BTreeSet;

use crate::{
    BoundProcedure, ProcedureAst, SrplProcedureContractMetadata, SrplProcedureIr,
    SrplResultStreamIr,
};

pub fn lower_bound_procedure(bound: BoundProcedure) -> AndromedaResult<SrplProcedureIr> {
    bound.signature.validate()?;
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
    })
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
}
