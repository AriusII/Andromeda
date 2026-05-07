use andromeda_catalog::{
    DefinitionBatchDependencyGraphHash, DefinitionBatchSourceHash, QualifiedName,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, ContractHash,
    ProcedureId, digest::Sha256,
};

use super::MAX_SRPL_DEFINITION_BATCH_PROCEDURES;
use super::dry_run::SrplProcedureDryRunManifest;

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
