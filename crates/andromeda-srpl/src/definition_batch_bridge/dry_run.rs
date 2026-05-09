use andromeda_catalog::{
    CatalogDefinition, CatalogMutationRecordKind, CatalogSystemDurableApplyReport,
    CatalogSystemStore, DefinitionBatch, DefinitionBatchDependencyGraphHash, DefinitionBatchId,
    DefinitionBatchPlan, DefinitionBatchSourceHash, DefinitionOperation,
};
use andromeda_definition_batch::{
    MAX_SRPL_DEFINITION_BATCH_PROCEDURES, SrplDefinitionBatchDiagnostic,
    SrplDefinitionBatchDryRunError, SrplDefinitionBatchSourceEvidence, SrplProcedureDryRunManifest,
    SrplProcedureSourceDigest,
};
use andromeda_error::AndromedaResult;
use andromeda_types::CatalogVersion;

use crate::{
    DiagnosticPhase, SrplDiagnostic, SrplProcedureContractMetadata,
    compile_narrow_procedure_definition,
};

/// One SRPL Procedure source prepared for DefinitionBatch dry-run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchProcedureSource {
    pub source: String,
    pub metadata: SrplProcedureContractMetadata,
}

impl SrplDefinitionBatchProcedureSource {
    pub fn new(source: impl Into<String>, metadata: SrplProcedureContractMetadata) -> Self {
        Self {
            source: source.into(),
            metadata,
        }
    }

    pub fn source_digest(&self) -> SrplProcedureSourceDigest {
        SrplProcedureSourceDigest::from_source(&self.source)
    }
}

/// Source-level request for building and dry-running a DefinitionBatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchDryRunRequest {
    pub batch_id: DefinitionBatchId,
    pub database_id: andromeda_types::DatabaseId,
    pub namespace_id: andromeda_types::NamespaceId,
    pub base_version: CatalogVersion,
    pub procedures: Vec<SrplDefinitionBatchProcedureSource>,
}

/// Successful all-or-nothing SRPL DefinitionBatch dry-run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchDryRunReport {
    pub definition_batch: DefinitionBatch,
    pub plan: DefinitionBatchPlan,
    pub definition_batch_source_hash: DefinitionBatchSourceHash,
    pub definition_batch_dependency_graph_hash: DefinitionBatchDependencyGraphHash,
    pub manifests: Vec<SrplProcedureDryRunManifest>,
    pub source_evidence: SrplDefinitionBatchSourceEvidence,
}

impl SrplDefinitionBatchDryRunReport {
    /// Applies this dry-run report through the catalog-owned durable
    /// DefinitionBatch adapter.
    ///
    /// This method is a compatibility wrapper around
    /// [`apply_srpl_definition_batch_report_to_catalog_store_durably`]. The
    /// durable apply path remains owned by `andromeda-catalog`; SRPL only
    /// supplies the already dry-run DefinitionBatch plus source-evidence hashes
    /// that must match before and after catalog publication.
    pub fn apply_to_catalog_store_durably<Append, Flush>(
        &self,
        store: &mut CatalogSystemStore,
        append: Append,
        flush_through: Flush,
    ) -> AndromedaResult<SrplDefinitionBatchDurableApplyReport>
    where
        Append: FnMut(CatalogMutationRecordKind, &[u8]) -> AndromedaResult<u64>,
        Flush: FnMut(u64) -> AndromedaResult<u64>,
    {
        apply_srpl_definition_batch_report_to_catalog_store_durably(
            self,
            store,
            append,
            flush_through,
        )
    }
}

/// Catalog-owned durable adapter for an SRPL DefinitionBatch dry-run report.
///
/// This function deliberately does not compile SRPL, open runtime storage, or
/// publish application-visible state itself. It delegates the durable mutation
/// to [`CatalogSystemStore::apply_definition_batch_durably`] and verifies that
/// the SRPL source evidence still matches the catalog source and dependency
/// graph hashes on both sides of that catalog-owned apply operation.
pub fn apply_srpl_definition_batch_report_to_catalog_store_durably<Append, Flush>(
    report: &SrplDefinitionBatchDryRunReport,
    store: &mut CatalogSystemStore,
    append: Append,
    flush_through: Flush,
) -> AndromedaResult<SrplDefinitionBatchDurableApplyReport>
where
    Append: FnMut(CatalogMutationRecordKind, &[u8]) -> AndromedaResult<u64>,
    Flush: FnMut(u64) -> AndromedaResult<u64>,
{
    let expected_source_hash = report.definition_batch.source_hash();
    let expected_dependency_graph_hash = report.definition_batch.dependency_graph_hash()?;
    report
        .source_evidence
        .validate_catalog_hashes(expected_source_hash, expected_dependency_graph_hash)?;

    let catalog_report =
        store.apply_definition_batch_durably(&report.definition_batch, append, flush_through)?;
    report.source_evidence.validate_catalog_hashes(
        catalog_report.source_hash,
        catalog_report.dependency_graph_hash,
    )?;

    Ok(SrplDefinitionBatchDurableApplyReport {
        catalog_report,
        source_evidence: report.source_evidence.clone(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchDurableApplyReport {
    pub catalog_report: CatalogSystemDurableApplyReport,
    pub source_evidence: SrplDefinitionBatchSourceEvidence,
}

/// Parses, binds, lowers, materializes, and dry-runs a source-defined
/// DefinitionBatch without requiring `andromeda-catalog` to depend on SRPL.
///
/// The function is side-effect free. It first compiles every source into a
/// materialized Procedure contract. If any source is rejected, no
/// DefinitionBatch is built and all source diagnostics are returned. Only once
/// every Procedure source is valid does it invoke catalog `dry_run()` against
/// the materialized Procedure manifests.
pub fn dry_run_srpl_definition_batch_sources(
    request: SrplDefinitionBatchDryRunRequest,
) -> Result<SrplDefinitionBatchDryRunReport, SrplDefinitionBatchDryRunError> {
    validate_request_bounds(&request)?;
    let SrplDefinitionBatchDryRunRequest {
        batch_id,
        database_id,
        namespace_id,
        base_version,
        procedures,
    } = request;

    let mut diagnostics = Vec::new();
    let mut operations = Vec::with_capacity(procedures.len());
    let mut manifests = Vec::with_capacity(procedures.len());

    for (source_index, procedure) in procedures.into_iter().enumerate() {
        compile_source_for_dry_run(
            source_index,
            procedure,
            &mut operations,
            &mut manifests,
            &mut diagnostics,
        );
    }

    if !diagnostics.is_empty() {
        return Err(SrplDefinitionBatchDryRunError::new(diagnostics));
    }

    build_dry_run_report(
        batch_id,
        database_id,
        namespace_id,
        base_version,
        operations,
        manifests,
    )
}

fn validate_request_bounds(
    request: &SrplDefinitionBatchDryRunRequest,
) -> Result<(), SrplDefinitionBatchDryRunError> {
    if request.procedures.is_empty() {
        return Err(SrplDefinitionBatchDryRunError::new(vec![
            SrplDefinitionBatchDiagnostic::batch(
                "SRPL DefinitionBatch dry-run requires at least one Procedure source",
            ),
        ]));
    }

    if request.procedures.len() > MAX_SRPL_DEFINITION_BATCH_PROCEDURES {
        return Err(SrplDefinitionBatchDryRunError::new(vec![
            SrplDefinitionBatchDiagnostic::batch(format!(
                "SRPL DefinitionBatch dry-run accepts at most {MAX_SRPL_DEFINITION_BATCH_PROCEDURES} Procedure sources"
            )),
        ]));
    }

    Ok(())
}

fn compile_source_for_dry_run(
    source_index: usize,
    procedure: SrplDefinitionBatchProcedureSource,
    operations: &mut Vec<DefinitionOperation>,
    manifests: &mut Vec<SrplProcedureDryRunManifest>,
    diagnostics: &mut Vec<SrplDefinitionBatchDiagnostic>,
) {
    let source_digest = procedure.source_digest();
    match compile_narrow_procedure_definition(&procedure.source, procedure.metadata) {
        Ok(CatalogDefinition::Procedure(contract)) => {
            materialize_procedure_operation(
                source_index,
                source_digest,
                contract,
                operations,
                manifests,
                diagnostics,
            );
        },
        Ok(other) => diagnostics.push(SrplDefinitionBatchDiagnostic::source(
            source_index,
            SrplDiagnostic::new(
                DiagnosticPhase::IrLowering,
                None,
                format!(
                    "SRPL source materialized as non-Procedure catalog object {:?}",
                    other.object_ref().kind
                ),
            ),
        )),
        Err(diagnostic) => {
            diagnostics.push(SrplDefinitionBatchDiagnostic::source(
                source_index,
                diagnostic,
            ));
        },
    }
}

fn materialize_procedure_operation(
    source_index: usize,
    source_digest: SrplProcedureSourceDigest,
    contract: andromeda_catalog::ProcedureContract,
    operations: &mut Vec<DefinitionOperation>,
    manifests: &mut Vec<SrplProcedureDryRunManifest>,
    diagnostics: &mut Vec<SrplDefinitionBatchDiagnostic>,
) {
    let binding = contract.binding();
    if let Err(error) = contract.validate_canonical_hash() {
        diagnostics.push(SrplDefinitionBatchDiagnostic::source_for_procedure(
            source_index,
            contract.object.name.clone(),
            SrplDiagnostic::new(
                DiagnosticPhase::IrLowering,
                None,
                format!("materialized Procedure contract is invalid: {error}"),
            ),
        ));
        return;
    }
    if let Err(error) = binding.validate() {
        diagnostics.push(SrplDefinitionBatchDiagnostic::source_for_procedure(
            source_index,
            contract.object.name.clone(),
            SrplDiagnostic::new(
                DiagnosticPhase::IrLowering,
                None,
                format!("materialized Procedure binding is invalid: {error}"),
            ),
        ));
        return;
    }

    manifests.push(SrplProcedureDryRunManifest {
        source_index,
        source_digest,
        procedure_name: contract.object.name.clone(),
        object_id: contract.object.object_id,
        binding,
        contract_hash: contract.contract_hash,
        input_count: contract.inputs.len(),
        result_stream_count: contract.result_streams.len(),
        result_stream_cardinalities: contract
            .result_streams
            .iter()
            .map(|stream| stream.cardinality)
            .collect(),
    });
    operations.push(DefinitionOperation::Create(CatalogDefinition::Procedure(
        contract,
    )));
}

fn build_dry_run_report(
    batch_id: DefinitionBatchId,
    database_id: andromeda_types::DatabaseId,
    namespace_id: andromeda_types::NamespaceId,
    base_version: CatalogVersion,
    operations: Vec<DefinitionOperation>,
    manifests: Vec<SrplProcedureDryRunManifest>,
) -> Result<SrplDefinitionBatchDryRunReport, SrplDefinitionBatchDryRunError> {
    let definition_batch = DefinitionBatch {
        batch_id,
        database_id,
        namespace_id,
        base_version,
        operations,
    };
    let plan = definition_batch.dry_run().map_err(|error| {
        SrplDefinitionBatchDryRunError::new(vec![SrplDefinitionBatchDiagnostic::batch(format!(
            "materialized DefinitionBatch was rejected by catalog dry-run: {error}"
        ))])
    })?;
    let definition_batch_source_hash = definition_batch.source_hash();
    let definition_batch_dependency_graph_hash =
        definition_batch.dependency_graph_hash().map_err(|error| {
            SrplDefinitionBatchDryRunError::new(vec![SrplDefinitionBatchDiagnostic::batch(
                format!(
                    "materialized DefinitionBatch dependency graph hash could not be computed: {error}"
                ),
            )])
        })?;
    let source_evidence = SrplDefinitionBatchSourceEvidence::from_manifests(
        definition_batch_source_hash,
        definition_batch_dependency_graph_hash,
        &manifests,
    )
    .map_err(|error| {
        SrplDefinitionBatchDryRunError::new(vec![SrplDefinitionBatchDiagnostic::batch(format!(
            "materialized SRPL source evidence was rejected: {error}"
        ))])
    })?;

    Ok(SrplDefinitionBatchDryRunReport {
        definition_batch,
        plan,
        definition_batch_source_hash,
        definition_batch_dependency_graph_hash,
        manifests,
        source_evidence,
    })
}
