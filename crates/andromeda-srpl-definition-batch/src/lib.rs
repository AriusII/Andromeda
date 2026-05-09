#![forbid(unsafe_code)]

//! SRPL source bridge for DefinitionBatch Procedure definitions.
//!
//! This crate integrates the SRPL owner crates with catalog DefinitionBatch
//! planning and durable apply evidence. It owns the source-level bridge without
//! making `andromeda-definition-batch` parse SRPL and without keeping the
//! implementation inside the `andromeda-srpl` compiler orchestration crate.
//!
//! 1. **Dry-run orchestration**: Bounded source-set compilation, Procedure
//!    manifest materialization, catalog DefinitionBatch dry-run, and durable
//!    apply evidence validation.
//! 2. **Source evidence**: Exact SRPL source digests bound to canonical,
//!    typed, hashed, versioned Procedure contracts and catalog batch hashes.
//! 3. **Diagnostics**: Deterministic source and batch rejection reporting.
//! 4. **Procedure definition conversion**: Staged parse, bind/lower, and
//!    Procedure contract materialization.
//!
//! ## Contract
//!
//! - **Input**: bounded SRPL Procedure source set.
//! - **Output**: `CatalogDefinition::Procedure` values inside a DefinitionBatch.
//! - **Properties**: deterministic typed contracts and stable source evidence.
//! - **Side Effects**: None for dry-run; durable apply delegates to catalog WAL
//!   append/flush callbacks before catalog visibility.

mod compiler;
mod dry_run;
mod procedure_definition;

pub use andromeda_definition_batch::{
    MAX_SRPL_DEFINITION_BATCH_PROCEDURES, SrplDefinitionBatchDiagnostic,
    SrplDefinitionBatchDryRunError, SrplDefinitionBatchSourceEvidence, SrplProcedureDryRunManifest,
    SrplProcedureSourceDigest, SrplProcedureSourceDigestEvidence,
};
pub use compiler::{
    INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, compile_inventory_reserve_stock_contract,
    compile_inventory_reserve_stock_contract_candidate,
    compile_narrow_procedure_contract_candidate, compile_narrow_procedure_definition,
    compile_narrow_procedure_definition_batch, compile_narrow_procedure_signature,
    inventory_reserve_stock_contract_metadata, lower_bound_procedure,
    lower_ir_to_catalog_definition,
};
pub use dry_run::{
    SrplDefinitionBatchDryRunReport, SrplDefinitionBatchDryRunRequest,
    SrplDefinitionBatchDurableApplyReport, SrplDefinitionBatchProcedureSource,
    dry_run_srpl_definition_batch_sources,
};
pub use procedure_definition::SrplProcedureDefinition;
