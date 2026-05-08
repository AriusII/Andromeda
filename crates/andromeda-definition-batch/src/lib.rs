#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Definition Batch

Runtime-free DefinitionBatch identity, import correlation, and taxonomy primitives.

This crate exposes stable identity/hash primitives, import correlation
identifiers, and taxonomy placeholders. It does not parse, bind, apply, roll
back, or publish definition changes. It does not claim release readiness,
serialize network or disk formats, or authorize any application-facing ad hoc
SQL surface.

Catalog publication remains outside this crate and must not become visible
without durable WAL.
"#]

mod identity;
mod taxonomy;

pub use identity::{DefinitionBatchId, DefinitionBatchImportId, DefinitionBatchSourceHash};
pub use taxonomy::{
    ALL_DEFINITION_BATCH_APPLY_BARRIERS, ALL_DEFINITION_BATCH_OPERATION_KINDS,
    ALL_DEFINITION_BATCH_PHASES, DEFINITION_BATCH_BARRIER_AUDIT_EVIDENCE_REQUIRED,
    DEFINITION_BATCH_BARRIER_DURABLE_WAL_REQUIRED,
    DEFINITION_BATCH_BARRIER_EXPLICIT_OPERATOR_AUTHORIZATION,
    DEFINITION_BATCH_OPERATION_CREATE_OBJECT, DEFINITION_BATCH_OPERATION_REPLACE_OBJECT,
    DEFINITION_BATCH_OPERATION_RETIRE_OBJECT, DEFINITION_BATCH_PHASE_BIND,
    DEFINITION_BATCH_PHASE_CONFLICT_CHECK, DEFINITION_BATCH_PHASE_DEPENDENCY_ORDER,
    DEFINITION_BATCH_PHASE_DRY_RUN, DEFINITION_BATCH_PHASE_PARSE,
    DEFINITION_BATCH_PHASE_WAL_PRECONDITION, DEFINITION_BATCH_SCHEMA_VERSION,
    DEFINITION_BATCH_TAXONOMY_ID, DefinitionBatchApplyBarrier, DefinitionBatchOperationKind,
    DefinitionBatchPhase, TaxonomyEntry, TaxonomyStatus,
};
