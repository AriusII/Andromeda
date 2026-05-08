/// Stable identifier for the DefinitionBatch placeholder taxonomy.
pub const DEFINITION_BATCH_TAXONOMY_ID: &str = "andromeda.definition_batch.v0";

/// Placeholder schema version for future DefinitionBatch ownership.
pub const DEFINITION_BATCH_SCHEMA_VERSION: u16 = 0;

pub const DEFINITION_BATCH_OPERATION_CREATE_OBJECT: &str = "operation.create_object";
pub const DEFINITION_BATCH_OPERATION_REPLACE_OBJECT: &str = "operation.replace_object";
pub const DEFINITION_BATCH_OPERATION_RETIRE_OBJECT: &str = "operation.retire_object";

pub const DEFINITION_BATCH_PHASE_PARSE: &str = "phase.parse";
pub const DEFINITION_BATCH_PHASE_BIND: &str = "phase.bind";
pub const DEFINITION_BATCH_PHASE_DEPENDENCY_ORDER: &str = "phase.dependency_order";
pub const DEFINITION_BATCH_PHASE_CONFLICT_CHECK: &str = "phase.conflict_check";
pub const DEFINITION_BATCH_PHASE_DRY_RUN: &str = "phase.dry_run";
pub const DEFINITION_BATCH_PHASE_WAL_PRECONDITION: &str = "phase.wal_precondition";

pub const DEFINITION_BATCH_BARRIER_DURABLE_WAL_REQUIRED: &str = "barrier.durable_wal_required";
pub const DEFINITION_BATCH_BARRIER_AUDIT_EVIDENCE_REQUIRED: &str =
    "barrier.audit_evidence_required";
pub const DEFINITION_BATCH_BARRIER_EXPLICIT_OPERATOR_AUTHORIZATION: &str =
    "barrier.explicit_operator_authorization";

/// Stability state for placeholder taxonomy entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaxonomyStatus {
    /// The identifier is reserved for future implementation work.
    Reserved,
}

/// Runtime-free taxonomy entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaxonomyEntry {
    /// Stable dotted identifier.
    pub id: &'static str,
    /// Human-readable label for planning and documentation.
    pub label: &'static str,
    /// Placeholder stability state.
    pub status: TaxonomyStatus,
}

pub type DefinitionBatchOperationKind = TaxonomyEntry;
pub type DefinitionBatchPhase = TaxonomyEntry;
pub type DefinitionBatchApplyBarrier = TaxonomyEntry;

pub const ALL_DEFINITION_BATCH_OPERATION_KINDS: &[DefinitionBatchOperationKind] = &[
    DefinitionBatchOperationKind {
        id: DEFINITION_BATCH_OPERATION_CREATE_OBJECT,
        label: "Create catalog object definition",
        status: TaxonomyStatus::Reserved,
    },
    DefinitionBatchOperationKind {
        id: DEFINITION_BATCH_OPERATION_REPLACE_OBJECT,
        label: "Replace catalog object definition",
        status: TaxonomyStatus::Reserved,
    },
    DefinitionBatchOperationKind {
        id: DEFINITION_BATCH_OPERATION_RETIRE_OBJECT,
        label: "Retire catalog object definition",
        status: TaxonomyStatus::Reserved,
    },
];

pub const ALL_DEFINITION_BATCH_PHASES: &[DefinitionBatchPhase] = &[
    DefinitionBatchPhase {
        id: DEFINITION_BATCH_PHASE_PARSE,
        label: "Parse submitted definition batch",
        status: TaxonomyStatus::Reserved,
    },
    DefinitionBatchPhase {
        id: DEFINITION_BATCH_PHASE_BIND,
        label: "Bind catalog names and contract references",
        status: TaxonomyStatus::Reserved,
    },
    DefinitionBatchPhase {
        id: DEFINITION_BATCH_PHASE_DEPENDENCY_ORDER,
        label: "Build deterministic dependency order",
        status: TaxonomyStatus::Reserved,
    },
    DefinitionBatchPhase {
        id: DEFINITION_BATCH_PHASE_CONFLICT_CHECK,
        label: "Detect definition conflicts",
        status: TaxonomyStatus::Reserved,
    },
    DefinitionBatchPhase {
        id: DEFINITION_BATCH_PHASE_DRY_RUN,
        label: "Produce dry-run report",
        status: TaxonomyStatus::Reserved,
    },
    DefinitionBatchPhase {
        id: DEFINITION_BATCH_PHASE_WAL_PRECONDITION,
        label: "Check durable WAL precondition",
        status: TaxonomyStatus::Reserved,
    },
];

pub const ALL_DEFINITION_BATCH_APPLY_BARRIERS: &[DefinitionBatchApplyBarrier] = &[
    DefinitionBatchApplyBarrier {
        id: DEFINITION_BATCH_BARRIER_DURABLE_WAL_REQUIRED,
        label: "Durable WAL required before visible publication",
        status: TaxonomyStatus::Reserved,
    },
    DefinitionBatchApplyBarrier {
        id: DEFINITION_BATCH_BARRIER_AUDIT_EVIDENCE_REQUIRED,
        label: "Audit evidence required before apply",
        status: TaxonomyStatus::Reserved,
    },
    DefinitionBatchApplyBarrier {
        id: DEFINITION_BATCH_BARRIER_EXPLICIT_OPERATOR_AUTHORIZATION,
        label: "Explicit operator authorization required",
        status: TaxonomyStatus::Reserved,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_groups_are_non_empty() {
        assert!(!ALL_DEFINITION_BATCH_OPERATION_KINDS.is_empty());
        assert!(!ALL_DEFINITION_BATCH_PHASES.is_empty());
        assert!(!ALL_DEFINITION_BATCH_APPLY_BARRIERS.is_empty());
    }

    #[test]
    fn taxonomy_id_is_reserved_v0() {
        assert_eq!(
            DEFINITION_BATCH_TAXONOMY_ID,
            "andromeda.definition_batch.v0"
        );
        assert_eq!(DEFINITION_BATCH_SCHEMA_VERSION, 0);
    }
}
