//! Minimal Procedure Store scaffold.
//!
//! The Procedure Store is the canonical, in-memory registry of executable
//! procedure identities and the contract metadata required to invoke them.
//! It is intentionally **not** an opaque runtime cache: every entry is
//! materialised from a validated [`ProcedureContract`] (or constructed from
//! the same typed building blocks) and the store exposes only typed
//! accessors. There is no ad hoc text query surface (no SQL, no command
//! string) — callers must address procedures by `ProcedureId` or
//! `QualifiedName`.
//!
//! The store also accepts [`InvocationDecisionRecord`] values, allowing every
//! invocation that touches a registered procedure to be associated with a
//! [`DecisionTrace`] for downstream audit. The store does not emit traces
//! itself; it is a passive evidence sink that enforces the binding between
//! evidence and the contract metadata that authorised the invocation.

use std::collections::BTreeMap;

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash,
    InvocationId, ProcedureId,
};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

use crate::{
    InMemoryProcedureFeedbackStore, ProcedureContract, ProcedureContractBinding, ProcedureFeedback,
    ProcedureFeedbackStore, ProtocolLayoutRef, QualifiedName, RecordOutcome, TransactionPolicy,
};

/// Default per-procedure capacity for advisory feedback evidence.  The
/// store is bounded so observed-outcome feedback cannot grow without
/// limit; chosen as a small power-of-two sample window.  The limit is
/// part of the doctrine: advisory evidence is *sampled*, never
/// exhaustively retained.
pub const PROCEDURE_FEEDBACK_CAPACITY_PER_PROCEDURE: usize = 64;

/// Canonical, durable identity + contract metadata for an executable
/// procedure. Holds only contract-derived data — never plan caches, runtime
/// state, or buffered results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureStoreEntry {
    pub procedure_id: ProcedureId,
    pub name: QualifiedName,
    pub binding: ProcedureContractBinding,
    pub protocol_layout: ProtocolLayoutRef,
    pub transaction_policy: TransactionPolicy,
    pub required_permissions: Vec<String>,
}

impl ProcedureStoreEntry {
    /// Project a validated [`ProcedureContract`] into a store entry.
    pub fn from_contract(contract: &ProcedureContract) -> AndromedaResult<Self> {
        contract.validate()?;
        let binding = contract.binding();
        Ok(Self {
            procedure_id: contract.procedure_id,
            name: contract.object.name.clone(),
            binding,
            protocol_layout: contract.protocol_layout,
            transaction_policy: contract.transaction_policy,
            required_permissions: contract.required_permissions.clone(),
        })
    }

    /// Validate the entry's internal consistency (delegates to the
    /// underlying contract building blocks).
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure store entry id must not be zero",
            ));
        }
        self.binding.validate()?;
        if self.binding.procedure_id != self.procedure_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store entry id and binding id must agree",
            ));
        }
        self.protocol_layout.validate()?;
        if self.required_permissions.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "procedure store entry must declare required permissions",
            ));
        }
        Ok(())
    }

    pub fn contract_hash(&self) -> ContractHash {
        self.binding.contract_hash
    }

    pub fn catalog_version(&self) -> CatalogVersion {
        self.binding.catalog_version
    }
}

/// Decision evidence attached to a single invocation. Every invocation that
/// reaches the store must carry a [`DecisionTrace`] explaining why it was
/// admitted, rejected, contract-validated, etc.
///
/// This struct is the *evidence shape* — it does not itself decide anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationDecisionRecord {
    pub invocation_id: InvocationId,
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub trace: DecisionTrace,
}

impl InvocationDecisionRecord {
    pub fn new(
        invocation_id: InvocationId,
        procedure_id: ProcedureId,
        contract_hash: ContractHash,
        catalog_version: CatalogVersion,
        trace: DecisionTrace,
    ) -> AndromedaResult<Self> {
        let record = Self {
            invocation_id,
            procedure_id,
            contract_hash,
            catalog_version,
            trace,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.invocation_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "invocation decision invocation id must not be zero",
            ));
        }
        if self.procedure_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "invocation decision procedure id must not be zero",
            ));
        }
        if self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "invocation decision contract hash must not be zero",
            ));
        }
        if self.catalog_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "invocation decision catalog version must not be zero",
            ));
        }
        if !self.trace.has_explanation() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "invocation decision trace must carry a non-empty reason",
            ));
        }
        Ok(())
    }

    pub fn trace_id(&self) -> TraceId {
        self.trace.trace_id
    }

    pub fn decision_kind(&self) -> CriticalDecisionKind {
        self.trace.decision
    }
}

/// Minimal in-memory Procedure Store.
///
/// Indexed by both [`ProcedureId`] and [`QualifiedName`] to enforce that
/// neither identity may collide. Re-registering the same exact entry is a
/// no-op (idempotent); any divergence is rejected.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProcedureStore {
    entries: BTreeMap<ProcedureId, ProcedureStoreEntry>,
    by_name: BTreeMap<QualifiedName, ProcedureId>,
    decisions: BTreeMap<ProcedureId, Vec<InvocationDecisionRecord>>,
    feedback: BTreeMap<ProcedureId, InMemoryProcedureFeedbackStore>,
}

impl ProcedureStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Register an entry. Returns [`ProcedureRegistration::Inserted`] for a
    /// new procedure or [`ProcedureRegistration::AlreadyRegistered`] if the
    /// exact same entry was previously registered. Any conflict (id collision
    /// with different metadata, name collision with a different id, or
    /// contract hash divergence for the same id) is rejected.
    pub fn register(
        &mut self,
        entry: ProcedureStoreEntry,
    ) -> AndromedaResult<ProcedureRegistration> {
        entry.validate()?;

        if let Some(existing) = self.entries.get(&entry.procedure_id) {
            if existing == &entry {
                return Ok(ProcedureRegistration::AlreadyRegistered);
            }
            if existing.binding.contract_hash != entry.binding.contract_hash {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure store rejects contract hash divergence for existing procedure id",
                ));
            }
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure store rejects re-registration with conflicting metadata",
            ));
        }

        if let Some(existing_id) = self.by_name.get(&entry.name) {
            if *existing_id != entry.procedure_id {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "procedure store rejects qualified name collision across procedure ids",
                ));
            }
        }

        self.by_name.insert(entry.name.clone(), entry.procedure_id);
        self.entries.insert(entry.procedure_id, entry);
        Ok(ProcedureRegistration::Inserted)
    }

    pub fn get(&self, procedure_id: ProcedureId) -> Option<&ProcedureStoreEntry> {
        self.entries.get(&procedure_id)
    }

    pub fn lookup_by_name(&self, name: &QualifiedName) -> Option<&ProcedureStoreEntry> {
        let id = self.by_name.get(name)?;
        self.entries.get(id)
    }

    pub fn entries(&self) -> impl Iterator<Item = &ProcedureStoreEntry> {
        self.entries.values()
    }

    /// Attach decision evidence to an existing procedure. The contract hash
    /// and catalog version on the record must match the registered binding,
    /// preventing evidence from being recorded against stale or mismatched
    /// contract identities.
    pub fn attach_invocation_decision(
        &mut self,
        record: InvocationDecisionRecord,
    ) -> AndromedaResult<()> {
        record.validate()?;
        let entry = self.entries.get(&record.procedure_id).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store cannot attach decision evidence for unknown procedure id",
            )
        })?;

        if entry.binding.contract_hash != record.contract_hash {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store decision evidence contract hash does not match registered binding",
            ));
        }
        if entry.binding.catalog_version != record.catalog_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store decision evidence catalog version does not match registered binding",
            ));
        }

        self.decisions
            .entry(record.procedure_id)
            .or_default()
            .push(record);
        Ok(())
    }

    pub fn invocation_decisions_for(
        &self,
        procedure_id: ProcedureId,
    ) -> &[InvocationDecisionRecord] {
        self.decisions
            .get(&procedure_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn invocation_decisions_for_invocation(
        &self,
        invocation_id: InvocationId,
    ) -> impl Iterator<Item = &InvocationDecisionRecord> {
        self.decisions
            .values()
            .flat_map(|records| records.iter())
            .filter(move |record| record.invocation_id == invocation_id)
    }

    pub fn total_recorded_decisions(&self) -> usize {
        self.decisions.values().map(Vec::len).sum()
    }

    /// Attach **advisory, non-authoritative** observed-outcome feedback
    /// to a registered procedure.
    ///
    /// Doctrine:
    /// - The feedback is evidence-only.
    ///   [`ProcedureFeedback::is_authoritative`] is hard-wired to
    ///   `false`; this method does not promote it.
    /// - The feedback's `procedure_id` must match a registered entry.
    /// - The feedback's `stats_version` must match the registered
    ///   binding's `stats_version`.  Feedback gathered against a
    ///   different stats snapshot is rejected so it cannot be silently
    ///   re-applied to the wrong snapshot.
    /// - Recording the same `feedback_id` with the same digest is an
    ///   idempotent [`RecordOutcome::Duplicate`].  Recording the same
    ///   `feedback_id` with a different digest is rejected as a
    ///   conflict — no silent overwrite.
    /// - Per-procedure retention is bounded by
    ///   [`PROCEDURE_FEEDBACK_CAPACITY_PER_PROCEDURE`].  Eviction is
    ///   deterministic (lowest `(issued_at, feedback_id)` first).
    pub fn attach_procedure_feedback(
        &mut self,
        feedback: ProcedureFeedback,
    ) -> AndromedaResult<RecordOutcome> {
        let entry = self.entries.get(&feedback.procedure_id()).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store cannot attach feedback for unknown procedure id",
            )
        })?;

        if entry.binding.stats_version != feedback.stats_version() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store feedback stats version does not match registered binding",
            ));
        }

        let bucket = self
            .feedback
            .entry(feedback.procedure_id())
            .or_insert_with(|| {
                InMemoryProcedureFeedbackStore::new(PROCEDURE_FEEDBACK_CAPACITY_PER_PROCEDURE)
            });

        bucket
            .record(feedback)
            .map_err(|err| AndromedaError::new(AndromedaErrorKind::Contract, format!("{err}")))
    }

    /// Read-only view of advisory feedback recorded for a procedure
    /// that is non-expired at `now`.  Returns an empty vector when no
    /// feedback is present.
    pub fn procedure_feedback_for_at(
        &self,
        procedure_id: ProcedureId,
        now: andromeda_core::EngineTimestamp,
    ) -> Vec<ProcedureFeedback> {
        match self.feedback.get(&procedure_id) {
            Some(bucket) => bucket.iter_for_procedure_at(procedure_id, now),
            None => Vec::new(),
        }
    }

    /// Drop expired feedback across every procedure.  Returns the total
    /// number of records removed.
    pub fn prune_expired_procedure_feedback(
        &mut self,
        now: andromeda_core::EngineTimestamp,
    ) -> usize {
        self.feedback
            .values_mut()
            .map(|bucket| bucket.prune_expired(now))
            .sum()
    }

    /// Total stored feedback records across every procedure.
    pub fn total_procedure_feedback(&self) -> usize {
        self.feedback.values().map(|bucket| bucket.len()).sum()
    }
}

/// Outcome of a [`ProcedureStore::register`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureRegistration {
    Inserted,
    AlreadyRegistered,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AccessMode, CatalogObjectRef, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy,
        ObjectKind, PolicyVersion, ProcedureErrorPolicy, ResultMetadataPolicy, StatsVersion,
    };
    use andromeda_core::CatalogObjectId;

    fn entry(id: u64, name: &str, hash_seed: u8) -> ProcedureStoreEntry {
        ProcedureStoreEntry {
            procedure_id: ProcedureId::new(id),
            name: QualifiedName::parse(name).unwrap(),
            binding: ProcedureContractBinding {
                procedure_id: ProcedureId::new(id),
                catalog_version: CatalogVersion::new(7),
                contract_hash: ContractHash::test_vector(hash_seed),
                stats_version: StatsVersion::new(1),
                policy_version: PolicyVersion::new([1; PolicyVersion::LEN]),
            },
            protocol_layout: ProtocolLayoutRef {
                descriptor_set_hash: ContractHash::test_vector(101),
                frame_envelope_hash: ContractHash::test_vector(202),
            },
            transaction_policy: TransactionPolicy {
                access_mode: AccessMode::ReadWrite,
                isolation: IsolationPolicy::Snapshot,
                retryable: false,
            },
            required_permissions: vec!["execute_procedure".to_string()],
        }
    }

    fn decision(invocation: u64, procedure_id: u64, hash_seed: u8) -> InvocationDecisionRecord {
        InvocationDecisionRecord::new(
            InvocationId::new(invocation),
            ProcedureId::new(procedure_id),
            ContractHash::test_vector(hash_seed),
            CatalogVersion::new(7),
            DecisionTrace {
                trace_id: TraceId::new(invocation as u128),
                decision: CriticalDecisionKind::ContractValidation,
                reason: "contract validated against registered binding".to_string(),
            },
        )
        .unwrap()
    }

    #[test]
    fn registration_inserts_new_procedure_and_is_idempotent_for_same_entry() {
        let mut store = ProcedureStore::new();
        let e = entry(1, "Inventory.ReserveStock", 42);

        assert_eq!(
            store.register(e.clone()).unwrap(),
            ProcedureRegistration::Inserted
        );
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.register(e.clone()).unwrap(),
            ProcedureRegistration::AlreadyRegistered
        );
        assert!(store.get(ProcedureId::new(1)).is_some());
        assert!(
            store
                .lookup_by_name(&QualifiedName::parse("Inventory.ReserveStock").unwrap())
                .is_some()
        );
    }

    #[test]
    fn registration_rejects_contract_hash_divergence_for_same_id() {
        let mut store = ProcedureStore::new();
        store
            .register(entry(1, "Inventory.ReserveStock", 42))
            .unwrap();
        let err = store
            .register(entry(1, "Inventory.ReserveStock", 43))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("contract hash divergence"));
    }

    #[test]
    fn registration_rejects_qualified_name_collision_across_ids() {
        let mut store = ProcedureStore::new();
        store
            .register(entry(1, "Inventory.ReserveStock", 42))
            .unwrap();
        let err = store
            .register(entry(2, "Inventory.ReserveStock", 99))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Catalog);
        assert!(err.message().contains("qualified name collision"));
    }

    #[test]
    fn registration_rejects_zero_procedure_id() {
        let mut store = ProcedureStore::new();
        let bad = entry(0, "Inventory.ReserveStock", 42);
        let err = store.register(bad).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Catalog);
    }

    #[test]
    fn decision_evidence_attaches_and_is_queryable_by_procedure_and_invocation() {
        let mut store = ProcedureStore::new();
        store
            .register(entry(1, "Inventory.ReserveStock", 42))
            .unwrap();

        let record = decision(101, 1, 42);
        store.attach_invocation_decision(record.clone()).unwrap();

        let by_procedure = store.invocation_decisions_for(ProcedureId::new(1));
        assert_eq!(by_procedure.len(), 1);
        assert_eq!(by_procedure[0], record);

        let by_invocation: Vec<_> = store
            .invocation_decisions_for_invocation(InvocationId::new(101))
            .cloned()
            .collect();
        assert_eq!(by_invocation, vec![record]);
        assert_eq!(store.total_recorded_decisions(), 1);
    }

    #[test]
    fn decision_evidence_rejects_unknown_procedure() {
        let mut store = ProcedureStore::new();
        let err = store
            .attach_invocation_decision(decision(101, 1, 42))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("unknown procedure id"));
    }

    #[test]
    fn decision_evidence_rejects_contract_hash_mismatch_with_binding() {
        let mut store = ProcedureStore::new();
        store
            .register(entry(1, "Inventory.ReserveStock", 42))
            .unwrap();
        let err = store
            .attach_invocation_decision(decision(101, 1, 99))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("contract hash"));
    }

    #[test]
    fn decision_evidence_rejects_catalog_version_drift() {
        let mut store = ProcedureStore::new();
        store
            .register(entry(1, "Inventory.ReserveStock", 42))
            .unwrap();
        let mut record = decision(101, 1, 42);
        record.catalog_version = CatalogVersion::new(8);
        let err = store.attach_invocation_decision(record).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("catalog version"));
    }

    #[test]
    fn decision_evidence_requires_non_empty_reason() {
        let bad = InvocationDecisionRecord::new(
            InvocationId::new(101),
            ProcedureId::new(1),
            ContractHash::test_vector(42),
            CatalogVersion::new(7),
            DecisionTrace {
                trace_id: TraceId::new(1),
                decision: CriticalDecisionKind::ContractValidation,
                reason: "   ".to_string(),
            },
        );
        let err = bad.unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("non-empty reason"));
    }

    #[test]
    fn store_round_trips_a_validated_procedure_contract() {
        let contract = ProcedureContract {
            object: CatalogObjectRef {
                object_id: CatalogObjectId::new(11),
                name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
                kind: ObjectKind::Procedure,
                catalog_version: CatalogVersion::new(7),
            },
            procedure_id: ProcedureId::new(1),
            contract_hash: ContractHash::test_vector(42),
            stats_version: StatsVersion::new(1),
            protocol_layout: ProtocolLayoutRef {
                descriptor_set_hash: ContractHash::test_vector(101),
                frame_envelope_hash: ContractHash::test_vector(202),
            },
            inputs: Vec::new(),
            structured_inputs: Vec::new(),
            result_streams: Vec::new(),
            required_permissions: vec!["execute_procedure".to_string()],
            transaction_policy: TransactionPolicy {
                access_mode: AccessMode::ReadWrite,
                isolation: IsolationPolicy::Snapshot,
                retryable: false,
            },
            compatibility_policy: CompatibilityPolicy::ExactHash,
            result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
            error_policy: ProcedureErrorPolicy {
                rollback_on_error: true,
                allowed_error_codes: Vec::new(),
            },
            multi_result_policy: MultiResultPolicy::SingleResultOnly,
        };

        let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
        let mut store = ProcedureStore::new();
        assert_eq!(
            store.register(entry).unwrap(),
            ProcedureRegistration::Inserted
        );
        assert_eq!(
            store.get(ProcedureId::new(1)).unwrap().contract_hash(),
            ContractHash::test_vector(42)
        );
    }

    fn make_window(issued: u64, expires: u64) -> crate::ValidityWindow {
        crate::ValidityWindow::new(
            andromeda_core::EngineTimestamp::from_unix_millis(issued),
            andromeda_core::EngineTimestamp::from_unix_millis(expires),
        )
        .expect("valid window")
    }

    fn feedback_for(feedback_id: u64, procedure_id: u64, stats_version: u64) -> ProcedureFeedback {
        ProcedureFeedback::new(
            crate::FeedbackId::new(feedback_id).expect("non-zero feedback id"),
            ProcedureId::new(procedure_id),
            Some([0xCD; 32]),
            StatsVersion::new(stats_version),
            crate::CompletionEvidence {
                status: crate::CompletionStatus::Committed,
                completion_code: Some(0),
                row_count: Some(3),
                durable_lsn: Some(42),
            },
            make_window(10, 1_000),
        )
        .expect("valid feedback")
    }

    #[test]
    fn procedure_feedback_attaches_to_registered_procedure() {
        let mut store = ProcedureStore::new();
        store
            .register(entry(1, "Inventory.ReserveStock", 42))
            .unwrap();

        let outcome = store
            .attach_procedure_feedback(feedback_for(1001, 1, 1))
            .expect("registered procedure accepts matching feedback");
        assert_eq!(outcome, RecordOutcome::Stored);

        // Idempotent re-attach is a deterministic Duplicate.
        let outcome = store
            .attach_procedure_feedback(feedback_for(1001, 1, 1))
            .expect("byte-identical feedback is idempotent");
        assert_eq!(outcome, RecordOutcome::Duplicate);
        assert_eq!(store.total_procedure_feedback(), 1);

        let visible = store.procedure_feedback_for_at(
            ProcedureId::new(1),
            andromeda_core::EngineTimestamp::from_unix_millis(50),
        );
        assert_eq!(visible.len(), 1);
        assert!(!visible[0].is_authoritative());
    }

    #[test]
    fn procedure_feedback_rejects_unknown_procedure() {
        let mut store = ProcedureStore::new();
        let err = store
            .attach_procedure_feedback(feedback_for(1001, 99, 1))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("unknown procedure id"));
        assert_eq!(store.total_procedure_feedback(), 0);
    }

    #[test]
    fn procedure_feedback_rejects_stats_version_mismatch_with_binding() {
        let mut store = ProcedureStore::new();
        store
            .register(entry(1, "Inventory.ReserveStock", 42))
            .unwrap();
        // Registered binding uses StatsVersion::new(1); supply 7 instead.
        let err = store
            .attach_procedure_feedback(feedback_for(1001, 1, 7))
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("stats version"));
        assert_eq!(store.total_procedure_feedback(), 0);
    }

    #[test]
    fn procedure_feedback_rejects_conflicting_record_with_same_id_but_different_digest() {
        let mut store = ProcedureStore::new();
        store
            .register(entry(1, "Inventory.ReserveStock", 42))
            .unwrap();
        store
            .attach_procedure_feedback(feedback_for(1001, 1, 1))
            .unwrap();

        let conflicting = ProcedureFeedback::new(
            crate::FeedbackId::new(1001).unwrap(),
            ProcedureId::new(1),
            Some([0xCD; 32]),
            StatsVersion::new(1),
            crate::CompletionEvidence {
                status: crate::CompletionStatus::Failed,
                completion_code: Some(7),
                row_count: None,
                durable_lsn: None,
            },
            make_window(10, 1_000),
        )
        .unwrap();

        let err = store.attach_procedure_feedback(conflicting).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().to_lowercase().contains("conflict"));
        assert_eq!(store.total_procedure_feedback(), 1);
    }

    #[test]
    fn procedure_feedback_prune_expired_drops_only_expired_records() {
        let mut store = ProcedureStore::new();
        store
            .register(entry(1, "Inventory.ReserveStock", 42))
            .unwrap();
        // Window 10..50 expires at t=50; window 10..1000 stays valid.
        let short = ProcedureFeedback::new(
            crate::FeedbackId::new(1).unwrap(),
            ProcedureId::new(1),
            None,
            StatsVersion::new(1),
            crate::CompletionEvidence {
                status: crate::CompletionStatus::Committed,
                completion_code: Some(0),
                row_count: Some(1),
                durable_lsn: Some(7),
            },
            make_window(10, 50),
        )
        .unwrap();
        store.attach_procedure_feedback(short).unwrap();
        store
            .attach_procedure_feedback(feedback_for(2, 1, 1))
            .unwrap();

        let removed = store.prune_expired_procedure_feedback(
            andromeda_core::EngineTimestamp::from_unix_millis(100),
        );
        assert_eq!(removed, 1);
        assert_eq!(store.total_procedure_feedback(), 1);
    }

    /// Doctrine guard: the Procedure Store must not expose an ad hoc SQL or
    /// raw-text query surface. Callers must address procedures by typed id
    /// or qualified name. This test scans this module's source for forbidden
    /// tokens that would indicate such a surface was introduced. The needles
    /// are built at runtime from halves so this test body is not itself a
    /// false positive.
    #[test]
    fn procedure_store_exposes_no_ad_hoc_sql_surface() {
        let source = include_str!("procedure_store.rs");
        let halves: &[(&str, &str)] = &[
            ("fn query_", "sql"),
            ("fn execute_", "sql"),
            ("fn raw_", "query"),
            ("raw_", "sql"),
            ("SE", "LECT "),
            ("INSERT ", "INTO"),
            ("EXECUTE ", "IMMEDIATE"),
            ("prepare_", "sql"),
        ];
        for (a, b) in halves {
            let needle = format!("{a}{b}");
            assert!(
                !source.contains(needle.as_str()),
                "procedure store source must not expose ad-hoc SQL surface token: {needle}"
            );
        }
    }
}
