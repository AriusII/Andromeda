use andromeda_digest::Sha256;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{
    ProcedureContractBinding, QualifiedName, ResultStreamCardinality,
};
use andromeda_srpl_diagnostics::{DiagnosticPhase, SrplDiagnostic};
use andromeda_types::{CatalogObjectId, ContractHash, ProcedureId};

use crate::{DefinitionBatchDependencyGraphHash, DefinitionBatchSourceHash};

/// Maximum SRPL Procedure sources accepted by one DefinitionBatch dry-run.
///
/// This keeps SRPL compilation and diagnostic aggregation bounded before the
/// bridge materializes catalog operations.
pub const MAX_SRPL_DEFINITION_BATCH_PROCEDURES: usize = 128;

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
