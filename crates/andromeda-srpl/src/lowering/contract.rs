//! Catalog contract materialization for lowered SRPL Procedure IR.

use andromeda_catalog::{
    CatalogDefinition, DefinitionBatch, DefinitionBatchId, DefinitionOperation,
    ProcedureContractCandidate, ResultStreamContract, inventory_reserve_stock_contract_candidate,
};
use andromeda_error::AndromedaResult;
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use crate::{SrplProcedureContractMetadata, SrplProcedureIr};

use super::{
    INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, compile_narrow_procedure_signature,
    validation::validate_declared_error_codes,
};

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

/// Compiles a narrow SRPL source to a raw single-operation [`DefinitionBatch`].
///
/// This helper preserves the historical compiler facade for tests and narrow
/// internal callers. Production DefinitionBatch preparation should prefer
/// [`crate::definition_batch_bridge::dry_run_srpl_definition_batch_sources`]
/// so source evidence, manifest validation, and dry-run diagnostics remain
/// attached before catalog visibility.
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
