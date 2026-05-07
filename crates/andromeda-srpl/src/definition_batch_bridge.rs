//! Bridge layer: SRPL source -> DefinitionBatch Procedure definitions.
//!
//! This module integrates the SRPL compiler with catalog DefinitionBatch
//! planning and durable apply evidence. It provides:
//!
//! 1. **SrplProcedureDefinition**: A typed container for SRPL Procedures at
//!    compilation stages.
//!
//! 2. **Source dry-run**: Parse, bind, lower, materialize, and dry-run an
//!    all-or-nothing set of SRPL Procedure sources.
//!
//! 3. **Source evidence**: Bind exact SRPL source digests to canonical,
//!    typed, hashed, versioned Procedure contracts and catalog batch hashes.
//!
//! ## Contract
//!
//! - **Input**: bounded SRPL Procedure source set.
//! - **Output**: `CatalogDefinition::Procedure` values inside a DefinitionBatch.
//! - **Properties**: deterministic typed contracts and stable source evidence.
//! - **Side Effects**: None (pure function)
//!
//! ## Error Handling
//!
//! Errors are categorized by compilation phase:
//! - **Syntax errors**: Lexer/parser failures
//! - **Bind errors**: Name/type validation failures
//! - **Compile errors**: Semantic validation failures
//! - **Manifest errors**: Contract materialization failures

use andromeda_catalog::{
    AccessMode, CatalogDefinition, CatalogMutationRecordKind, CatalogSystemDurableApplyReport,
    CatalogSystemStore, CompatibilityPolicy, DefinitionBatch, DefinitionBatchDependencyGraphHash,
    DefinitionBatchId, DefinitionBatchPlan, DefinitionBatchSourceHash, DefinitionOperation,
    IsolationPolicy, MultiResultPolicy, ProcedureContractBinding, ProcedureErrorPolicy,
    ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy, ResultStreamCardinality, StatsVersion,
    TransactionPolicy,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ContractHash, ProcedureId, digest::Sha256,
};

use crate::{
    DiagnosticPhase, ProcedureAst, SrplDiagnostic, SrplProcedureContractMetadata, SrplProcedureIr,
    SrplSource, compile_narrow_procedure_definition, lowering::lower_bound_procedure,
    lowering::lower_ir_to_catalog_definition, procedure_compiler::parse_procedure_signature,
};

/// Maximum SRPL Procedure sources accepted by one DefinitionBatch dry-run.
///
/// This keeps SRPL compilation and diagnostic aggregation bounded before the
/// bridge materializes catalog operations.
pub const MAX_SRPL_DEFINITION_BATCH_PROCEDURES: usize = 128;

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

/// Exact SRPL source-text digest carried by the SRPL DefinitionBatch bridge.
///
/// This is intentionally separate from the catalog `DefinitionBatchSourceHash`.
/// The source digest binds the original SRPL text accepted by the compiler,
/// while the catalog source hash binds the ordered materialized
/// `DefinitionBatch` after SRPL has lowered to canonical Procedure contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SrplProcedureSourceDigest([u8; Self::LEN]);

impl SrplProcedureSourceDigest {
    pub const LEN: usize = 32;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub fn from_source(source: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"andromeda.srpl.procedure-source.v1.sha256");
        hasher.update(&(source.len() as u64).to_le_bytes());
        hasher.update(source.as_bytes());
        Self::new(hasher.finalize())
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

/// Source-level request for building and dry-running a DefinitionBatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchDryRunRequest {
    pub batch_id: DefinitionBatchId,
    pub database_id: andromeda_core::DatabaseId,
    pub namespace_id: andromeda_core::NamespaceId,
    pub base_version: CatalogVersion,
    pub procedures: Vec<SrplDefinitionBatchProcedureSource>,
}

/// Materialized Procedure manifest produced by the SRPL dry-run path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureDryRunManifest {
    pub source_index: usize,
    pub source_digest: SrplProcedureSourceDigest,
    pub procedure_name: QualifiedName,
    pub object_id: CatalogObjectId,
    pub binding: ProcedureContractBinding,
    pub contract_hash: ContractHash,
    pub input_count: usize,
    pub result_stream_count: usize,
    pub result_stream_cardinalities: Vec<ResultStreamCardinality>,
}

/// Source digest binding that can travel with catalog apply/durable evidence
/// without making raw SRPL source part of runtime system truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureSourceDigestEvidence {
    pub source_index: usize,
    pub source_digest: SrplProcedureSourceDigest,
    pub procedure_name: QualifiedName,
    pub object_id: CatalogObjectId,
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
}

impl SrplProcedureSourceDigestEvidence {
    pub fn from_manifest(manifest: &SrplProcedureDryRunManifest) -> Self {
        Self {
            source_index: manifest.source_index,
            source_digest: manifest.source_digest,
            procedure_name: manifest.procedure_name.clone(),
            object_id: manifest.object_id,
            procedure_id: manifest.binding.procedure_id,
            contract_hash: manifest.contract_hash,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.source_digest.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL Procedure source digest evidence must not be zero",
            ));
        }

        if self.object_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL Procedure source digest evidence object id must not be zero",
            ));
        }

        if self.procedure_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL Procedure source digest evidence procedure id must not be zero",
            ));
        }

        if self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL Procedure source digest evidence contract hash must not be zero",
            ));
        }

        Ok(())
    }
}

/// Batch-level SRPL source evidence bound to the materialized catalog batch.
///
/// This is an evidence envelope, not a runtime execution dependency: Procedure
/// execution remains bound to the canonical contract hash and catalog version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchSourceEvidence {
    pub definition_batch_source_hash: DefinitionBatchSourceHash,
    pub definition_batch_dependency_graph_hash: DefinitionBatchDependencyGraphHash,
    pub procedures: Vec<SrplProcedureSourceDigestEvidence>,
}

impl SrplDefinitionBatchSourceEvidence {
    pub fn from_manifests(
        definition_batch_source_hash: DefinitionBatchSourceHash,
        definition_batch_dependency_graph_hash: DefinitionBatchDependencyGraphHash,
        manifests: &[SrplProcedureDryRunManifest],
    ) -> AndromedaResult<Self> {
        let evidence = Self {
            definition_batch_source_hash,
            definition_batch_dependency_graph_hash,
            procedures: manifests
                .iter()
                .map(SrplProcedureSourceDigestEvidence::from_manifest)
                .collect(),
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.definition_batch_source_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL DefinitionBatch source evidence must include a nonzero catalog source hash",
            ));
        }

        if self.definition_batch_dependency_graph_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL DefinitionBatch source evidence must include a nonzero dependency graph hash",
            ));
        }

        if self.procedures.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL DefinitionBatch source evidence must include at least one Procedure digest",
            ));
        }

        if self.procedures.len() > MAX_SRPL_DEFINITION_BATCH_PROCEDURES {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!(
                    "SRPL DefinitionBatch source evidence accepts at most {MAX_SRPL_DEFINITION_BATCH_PROCEDURES} Procedure digests"
                ),
            ));
        }

        for procedure in &self.procedures {
            procedure.validate()?;
        }

        Ok(())
    }

    pub fn validate_catalog_hashes(
        &self,
        source_hash: DefinitionBatchSourceHash,
        dependency_graph_hash: DefinitionBatchDependencyGraphHash,
    ) -> AndromedaResult<()> {
        self.validate()?;

        if self.definition_batch_source_hash != source_hash {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL DefinitionBatch source evidence must match the applied catalog source hash",
            ));
        }

        if self.definition_batch_dependency_graph_hash != dependency_graph_hash {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL DefinitionBatch source evidence must match the applied dependency graph hash",
            ));
        }

        Ok(())
    }
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
        let catalog_report =
            store.apply_definition_batch_durably(&self.definition_batch, append, flush_through)?;
        self.source_evidence.validate_catalog_hashes(
            catalog_report.source_hash,
            catalog_report.dependency_graph_hash,
        )?;

        Ok(SrplDefinitionBatchDurableApplyReport {
            catalog_report,
            source_evidence: self.source_evidence.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchDurableApplyReport {
    pub catalog_report: CatalogSystemDurableApplyReport,
    pub source_evidence: SrplDefinitionBatchSourceEvidence,
}

/// A diagnostic emitted while compiling one SRPL source or validating the
/// resulting DefinitionBatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchDiagnostic {
    pub source_index: Option<usize>,
    pub procedure_name: Option<QualifiedName>,
    pub diagnostic: SrplDiagnostic,
}

impl SrplDefinitionBatchDiagnostic {
    pub fn source(source_index: usize, diagnostic: SrplDiagnostic) -> Self {
        Self {
            source_index: Some(source_index),
            procedure_name: None,
            diagnostic,
        }
    }

    pub fn source_for_procedure(
        source_index: usize,
        procedure_name: QualifiedName,
        diagnostic: SrplDiagnostic,
    ) -> Self {
        Self {
            source_index: Some(source_index),
            procedure_name: Some(procedure_name),
            diagnostic,
        }
    }

    pub fn batch(message: impl Into<String>) -> Self {
        Self {
            source_index: None,
            procedure_name: None,
            diagnostic: SrplDiagnostic::new(DiagnosticPhase::SemanticValidation, None, message),
        }
    }
}

/// Deterministic rejection for an SRPL DefinitionBatch source dry-run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchDryRunError {
    pub diagnostics: Vec<SrplDefinitionBatchDiagnostic>,
}

impl SrplDefinitionBatchDryRunError {
    pub fn new(diagnostics: Vec<SrplDefinitionBatchDiagnostic>) -> Self {
        Self { diagnostics }
    }

    pub fn into_andromeda_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Srpl, self.to_string())
    }
}

impl std::fmt::Display for SrplDefinitionBatchDryRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SRPL DefinitionBatch dry-run rejected {} diagnostic(s)",
            self.diagnostics.len()
        )?;
        for diagnostic in &self.diagnostics {
            let source = diagnostic
                .source_index
                .map(|index| format!("source[{index}]"))
                .unwrap_or_else(|| "batch".to_string());
            let procedure = diagnostic
                .procedure_name
                .as_ref()
                .map(|name| format!(" {}", name.as_catalog_path()))
                .unwrap_or_default();
            write!(
                f,
                "; {source}{procedure}: {}",
                diagnostic.diagnostic.message
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for SrplDefinitionBatchDryRunError {}

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

    let mut diagnostics = Vec::new();
    let mut operations = Vec::with_capacity(request.procedures.len());
    let mut manifests = Vec::with_capacity(request.procedures.len());

    for (source_index, procedure) in request.procedures.into_iter().enumerate() {
        let source_digest = procedure.source_digest();
        match compile_narrow_procedure_definition(&procedure.source, procedure.metadata) {
            Ok(CatalogDefinition::Procedure(contract)) => {
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
                    continue;
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
                    continue;
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
            }
        }
    }

    if !diagnostics.is_empty() {
        return Err(SrplDefinitionBatchDryRunError::new(diagnostics));
    }

    let definition_batch = DefinitionBatch {
        batch_id: request.batch_id,
        database_id: request.database_id,
        namespace_id: request.namespace_id,
        base_version: request.base_version,
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

/// A staged SRPL procedure definition during compilation.
///
/// This type tracks the SRPL source through various compilation stages,
/// enabling diagnostics and error recovery at each phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureDefinition {
    /// The original SRPL source code
    pub srpl_source: String,

    /// Parsed AST (populated after lexer/parser stage)
    pub parsed_ast: Option<ProcedureAst>,

    /// Compiled IR (populated after binder/lowering stage)
    pub compiled_ir: Option<SrplProcedureIr>,
}

impl SrplProcedureDefinition {
    /// Create a new definition from SRPL source.
    pub fn from_source(srpl_source: String) -> Self {
        Self {
            srpl_source,
            parsed_ast: None,
            compiled_ir: None,
        }
    }

    /// Parse SRPL source to AST.
    ///
    /// This is the first compilation phase: lexer → parser.
    /// On success, populates `parsed_ast`. On failure, returns a diagnostic.
    pub fn parse(&mut self) -> AndromedaResult<()> {
        if let Some(diagnostic) = SrplSource::new(&self.srpl_source)
            .forbidden_construct_diagnostics()
            .into_iter()
            .next()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!(
                    "SRPL core-language rejection during parsing: {:?}: {} at {:?}",
                    diagnostic.phase, diagnostic.message, diagnostic.location
                ),
            ));
        }

        let ast = parse_procedure_signature(&self.srpl_source).map_err(|diagnostic| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!(
                    "SRPL syntax error during parsing: {} at {:?}",
                    diagnostic.message, diagnostic.location
                ),
            )
        })?;
        self.parsed_ast = Some(ast);
        Ok(())
    }

    /// Bind AST to typed procedure and lower to IR.
    ///
    /// This is the second compilation phase: bind → lower.
    /// Requires that `parse()` has completed successfully.
    /// On success, populates `compiled_ir`. On failure, returns a diagnostic.
    pub fn bind_and_lower(&mut self) -> AndromedaResult<()> {
        let ast = self.parsed_ast.clone().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "cannot bind and lower before parsing",
            )
        })?;

        let bound = crate::binder::bind_procedure(ast).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!("SRPL binding error: {}", e),
            )
        })?;

        let ir = lower_bound_procedure(bound).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!("SRPL lowering error: {}", e),
            )
        })?;

        self.compiled_ir = Some(ir);
        Ok(())
    }

    /// Convert compiled IR into a CatalogProcedureDefinition.
    ///
    /// This is the final phase: manifest generation and catalog conversion.
    /// Requires that `bind_and_lower()` has completed successfully.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The catalog object ID for this procedure
    /// * `procedure_id` - The procedure ID for this procedure
    /// * `next_version` - The catalog version after this definition is applied
    /// * `database_id` - The database ID (for qualified naming)
    /// * `namespace_id` - The namespace ID (for qualified naming)
    ///
    /// # Returns
    ///
    /// A CatalogDefinition::Procedure with materialized contract.
    pub fn to_catalog_procedure_def(
        &self,
        object_id: CatalogObjectId,
        procedure_id: ProcedureId,
        next_version: CatalogVersion,
    ) -> AndromedaResult<CatalogDefinition> {
        self.to_catalog_procedure_def_with_metadata(SrplProcedureContractMetadata {
            object_id,
            procedure_id,
            catalog_version: next_version,
            stats_version: StatsVersion::new(1),
            protocol_layout: ProtocolLayoutRef {
                descriptor_set_hash: ContractHash::test_vector(0xA1),
                frame_envelope_hash: ContractHash::test_vector(0xA2),
            },
            structured_inputs: Vec::new(),
            required_permissions: vec!["procedure.Execute".to_string()], // Default permission
            transaction_policy: TransactionPolicy {
                access_mode: AccessMode::ReadWrite,
                isolation: IsolationPolicy::Serializable,
                retryable: false,
            },
            compatibility_policy: CompatibilityPolicy::ExactHash,
            result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
            error_policy: ProcedureErrorPolicy {
                rollback_on_error: true,
                allowed_error_codes: Vec::new(),
            },
            multi_result_policy: MultiResultPolicy::SingleResultOnly,
        })
    }

    /// Convert compiled IR into a CatalogProcedureDefinition with explicit
    /// catalog metadata.
    ///
    /// This is the preferred staged materialization path because it shares the
    /// same SRPL → ProcedureContract lowerer as the DefinitionBatch bridge.
    pub fn to_catalog_procedure_def_with_metadata(
        &self,
        metadata: SrplProcedureContractMetadata,
    ) -> AndromedaResult<CatalogDefinition> {
        let ir = self.compiled_ir.clone().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "cannot materialize catalog definition before bind_and_lower",
            )
        })?;

        lower_ir_to_catalog_definition(ir, metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srpl_procedure_definition_from_source_creates_empty_stages() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string();
        let def = SrplProcedureDefinition::from_source(source.clone());

        assert_eq!(def.srpl_source, source);
        assert!(def.parsed_ast.is_none());
        assert!(def.compiled_ir.is_none());
    }

    #[test]
    fn srpl_procedure_definition_parse_populates_ast() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string();
        let mut def = SrplProcedureDefinition::from_source(source);

        assert!(def.parse().is_ok());
        assert!(def.parsed_ast.is_some());
    }

    #[test]
    fn srpl_procedure_definition_parse_syntax_error() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns R many ();"
            .to_string();
        let mut def = SrplProcedureDefinition::from_source(source);

        let result = def.parse();
        assert!(result.is_err());
    }

    #[test]
    fn srpl_procedure_definition_bind_and_lower_requires_parse() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string();
        let mut def = SrplProcedureDefinition::from_source(source);

        let result = def.bind_and_lower();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("before parsing"));
    }

    #[test]
    fn srpl_procedure_definition_bind_and_lower_populates_ir() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string();
        let mut def = SrplProcedureDefinition::from_source(source);

        assert!(def.parse().is_ok());
        assert!(def.bind_and_lower().is_ok());
        assert!(def.compiled_ir.is_some());
    }

    #[test]
    fn srpl_procedure_definition_into_catalog_requires_ir() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string();
        let def = SrplProcedureDefinition::from_source(source);

        let result = def.to_catalog_procedure_def(
            CatalogObjectId::new(1),
            ProcedureId::new(1),
            CatalogVersion::new(1),
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("before bind_and_lower")
        );
    }
}
