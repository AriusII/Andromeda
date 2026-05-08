//! Procedure contract materialization from lowered SRPL IR.

use std::collections::BTreeSet;

use andromeda_contract::{
    CatalogObjectRef, ObjectKind, ProcedureContractCandidate, ResultStreamContract,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_srpl_ir::{
    ProcedureSignature, ResultContract, SrplBusinessOperationKindIr, SrplProcedureBodyIr,
    SrplProcedureContractMetadata, SrplProcedureIr,
};

/// Lowers a [`SrplProcedureIr`] and its [`SrplProcedureContractMetadata`] to a
/// [`ProcedureContractCandidate`] ready for catalog insertion.
pub fn lower_ir_to_contract_candidate(
    ir: SrplProcedureIr,
    metadata: SrplProcedureContractMetadata,
) -> AndromedaResult<ProcedureContractCandidate> {
    let signature = ProcedureSignature {
        name: ir.name.clone(),
        accepts: ir.inputs.clone(),
        returns: ir
            .result_streams
            .iter()
            .map(|result| ResultContract {
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
                cardinality: result.cardinality.into(),
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

fn validate_declared_error_codes(
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
