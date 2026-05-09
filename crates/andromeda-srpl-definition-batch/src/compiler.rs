use andromeda_business_fixtures::inventory_reserve_stock_contract_candidate;
use andromeda_catalog_store::CatalogDefinition;
use andromeda_definition_batch::{DefinitionBatch, DefinitionBatchId, DefinitionOperation};
use andromeda_error::AndromedaResult;
use andromeda_procedure_contract::{ProcedureContract, ProcedureContractCandidate};
use andromeda_srpl_binder::{BoundProcedure, bind_procedure, validate_ast_names_for_diagnostics};
use andromeda_srpl_diagnostics::{
    DiagnosticPhase, SrplDiagnostic, SrplSource, enrich_source_diagnostic,
};
use andromeda_srpl_ir::{SrplProcedureContractMetadata, SrplProcedureIr};
use andromeda_srpl_lowering::BoundProcedureLoweringInput;
use andromeda_srpl_parser::parse_procedure_signature;
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

pub fn lower_bound_procedure(bound: BoundProcedure) -> AndromedaResult<SrplProcedureIr> {
    andromeda_srpl_lowering::lower_bound_procedure(BoundProcedureLoweringInput {
        signature: bound.signature,
        body: bound.body,
    })
}

pub fn compile_narrow_procedure_signature(source: &str) -> Result<SrplProcedureIr, SrplDiagnostic> {
    let srpl_source = SrplSource::new(source);
    if let Some(diagnostic) = srpl_source
        .forbidden_construct_diagnostics()
        .into_iter()
        .next()
    {
        return Err(enrich_source_diagnostic(source, diagnostic, None));
    }

    let ast = parse_procedure_signature(source)
        .map_err(|diagnostic| enrich_source_diagnostic(source, diagnostic, None))?;
    let procedure_name = ast.name.value.as_catalog_path();
    validate_ast_names_for_diagnostics(&ast, source)?;
    let bound = bind_procedure(ast).map_err(|error| {
        SrplDiagnostic::new(
            DiagnosticPhase::Binding,
            None,
            format!("{}; procedure {}", error, procedure_name),
        )
    })?;
    let procedure_name = bound.signature.name.as_catalog_path();
    lower_bound_procedure(bound).map_err(|error| {
        SrplDiagnostic::new(
            DiagnosticPhase::IrLowering,
            None,
            format!("{}; procedure {}", error, procedure_name),
        )
    })
}

pub fn compile_narrow_procedure_contract_candidate(
    source: &str,
    metadata: SrplProcedureContractMetadata,
) -> Result<ProcedureContractCandidate, SrplDiagnostic> {
    let ir = compile_narrow_procedure_signature(source)?;
    andromeda_srpl_lowering::lower_ir_to_contract_candidate(ir, metadata)
        .map_err(|error| SrplDiagnostic::new(DiagnosticPhase::IrLowering, None, error.to_string()))
}

pub fn lower_ir_to_catalog_definition(
    ir: SrplProcedureIr,
    metadata: SrplProcedureContractMetadata,
) -> AndromedaResult<CatalogDefinition> {
    let contract =
        andromeda_srpl_lowering::lower_ir_to_contract_candidate(ir, metadata)?.materialize()?;
    Ok(CatalogDefinition::Procedure(contract))
}

pub fn compile_narrow_procedure_definition(
    source: &str,
    metadata: SrplProcedureContractMetadata,
) -> Result<CatalogDefinition, SrplDiagnostic> {
    let ir = compile_narrow_procedure_signature(source)?;
    lower_ir_to_catalog_definition(ir, metadata)
        .map_err(|error| SrplDiagnostic::new(DiagnosticPhase::IrLowering, None, error.to_string()))
}

pub fn compile_narrow_procedure_definition_batch(
    source: &str,
    metadata: SrplProcedureContractMetadata,
    batch_id: DefinitionBatchId,
    database_id: DatabaseId,
    namespace_id: NamespaceId,
    base_version: CatalogVersion,
) -> Result<DefinitionBatch, SrplDiagnostic> {
    let definition = compile_narrow_procedure_definition(source, metadata)?;
    Ok(DefinitionBatch {
        batch_id,
        database_id,
        namespace_id,
        base_version,
        operations: vec![DefinitionOperation::Create(definition)],
    })
}

/// Canonical SRPL source for `Inventory.ReserveStock`.
pub const INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE: &str = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId and Stock.AvailableQuantity >= Quantity else fail InsufficientStock; update Inventory.ProductStock set AvailableQuantity = Stock.AvailableQuantity - Quantity where ProductId = Stock.ProductId affected rows 1; return Reservation (Reserved); end;";

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
) -> Result<ProcedureContractCandidate, SrplDiagnostic> {
    compile_narrow_procedure_contract_candidate(
        INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE,
        inventory_reserve_stock_contract_metadata(catalog_version),
    )
}

/// Compiles the canonical `Inventory.ReserveStock` SRPL source to a
/// materialized [`ProcedureContract`].
pub fn compile_inventory_reserve_stock_contract(
    catalog_version: CatalogVersion,
) -> Result<ProcedureContract, SrplDiagnostic> {
    compile_inventory_reserve_stock_contract_candidate(catalog_version)?
        .materialize()
        .map_err(|error| SrplDiagnostic::new(DiagnosticPhase::IrLowering, None, error.to_string()))
}
