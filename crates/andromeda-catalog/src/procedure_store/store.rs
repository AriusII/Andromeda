use std::collections::BTreeMap;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_time::EngineTimestamp;
use andromeda_types::{InvocationId, ProcedureId};

use crate::{
    InMemoryProcedureFeedbackStore, ProcedureContractBinding, ProcedureFeedback,
    ProcedureFeedbackStore, QualifiedName, RecordOutcome,
};

use super::{
    decision::InvocationDecisionRecord, entry::ProcedureStoreEntry,
    registration::ProcedureRegistration, runtime::InvocationRuntimeRecord,
    runtime::InvocationRuntimeRecordOutcome,
};

/// Default per-procedure capacity for advisory feedback evidence.  The
/// store is bounded so observed-outcome feedback cannot grow without
/// limit; chosen as a small power-of-two sample window.  The limit is
/// part of the doctrine: advisory evidence is *sampled*, never
/// exhaustively retained.
pub const PROCEDURE_FEEDBACK_CAPACITY_PER_PROCEDURE: usize = 64;

/// In-memory Procedure Store contract and evidence index.
///
/// Indexed by both [`ProcedureId`] and [`QualifiedName`] to enforce that
/// neither identity may collide. Re-registering the same exact entry is a
/// no-op (idempotent); any divergence is rejected.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProcedureStore {
    entries: BTreeMap<ProcedureId, ProcedureStoreEntry>,
    by_name: BTreeMap<QualifiedName, ProcedureId>,
    decisions: BTreeMap<ProcedureId, Vec<InvocationDecisionRecord>>,
    runtime_records: BTreeMap<ProcedureId, Vec<InvocationRuntimeRecord>>,
    runtime_by_invocation: BTreeMap<InvocationId, ProcedureId>,
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
            if existing.binding != entry.binding {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure store rejects binding divergence for existing procedure id",
                ));
            }
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure store rejects re-registration with conflicting metadata",
            ));
        }

        if let Some(existing_id) = self.by_name.get(&entry.name)
            && *existing_id != entry.procedure_id
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure store rejects qualified name collision across procedure ids",
            ));
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
        if !record.is_authoritative_decision()
            || record.is_observed_feedback()
            || record.can_select_plan_alone()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store decision boundary accepts authoritative decision evidence only",
            ));
        }
        let entry = self.entries.get(&record.procedure_id()).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store cannot attach decision evidence for unknown procedure id",
            )
        })?;

        validate_decision_binding(entry, record.binding())?;

        self.decisions
            .entry(record.procedure_id())
            .or_default()
            .push(record);
        Ok(())
    }

    pub fn invocation_decisions_for(
        &self,
        procedure_id: ProcedureId,
    ) -> &[InvocationDecisionRecord] {
        match self.decisions.get(&procedure_id) {
            Some(records) => records.as_slice(),
            None => &[],
        }
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

    /// Attach terminal runtime evidence to an existing procedure.
    ///
    /// A runtime record is accepted only when it binds to the exact
    /// registered `ProcedureContractBinding`: `ProcedureId`, `ContractHash`,
    /// `CatalogVersion`, `StatsVersion`, and `PolicyVersion` must all match.
    /// Each invocation may have only one terminal runtime record. Re-attaching
    /// the exact same record is idempotent; divergent evidence for the same
    /// invocation is rejected.
    pub fn attach_invocation_runtime(
        &mut self,
        record: InvocationRuntimeRecord,
    ) -> AndromedaResult<InvocationRuntimeRecordOutcome> {
        record.validate()?;
        if !record.is_observed_feedback()
            || record.is_authoritative_decision()
            || record.can_select_plan_alone()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store runtime boundary accepts observed feedback evidence only",
            ));
        }
        let entry = self.entries.get(&record.procedure_id()).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store cannot attach runtime evidence for unknown procedure id",
            )
        })?;

        validate_runtime_binding(entry, record.binding())?;

        if let Some(existing_procedure_id) = self.runtime_by_invocation.get(&record.invocation_id) {
            let existing = self
                .runtime_records
                .get(existing_procedure_id)
                .and_then(|records| {
                    records
                        .iter()
                        .find(|candidate| candidate.invocation_id == record.invocation_id)
                })
                .ok_or_else(|| {
                    AndromedaError::new(
                        AndromedaErrorKind::Internal,
                        "procedure store runtime invocation index is inconsistent",
                    )
                })?;

            if existing == &record {
                return Ok(InvocationRuntimeRecordOutcome::Duplicate);
            }
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store rejects conflicting terminal runtime evidence for invocation",
            ));
        }

        self.runtime_by_invocation
            .insert(record.invocation_id, record.procedure_id());
        self.runtime_records
            .entry(record.procedure_id())
            .or_default()
            .push(record);
        Ok(InvocationRuntimeRecordOutcome::Stored)
    }

    pub fn invocation_runtime_for(&self, procedure_id: ProcedureId) -> &[InvocationRuntimeRecord] {
        match self.runtime_records.get(&procedure_id) {
            Some(records) => records.as_slice(),
            None => &[],
        }
    }

    pub fn invocation_runtime_for_invocation(
        &self,
        invocation_id: InvocationId,
    ) -> Option<&InvocationRuntimeRecord> {
        let procedure_id = self.runtime_by_invocation.get(&invocation_id)?;
        self.runtime_records
            .get(procedure_id)?
            .iter()
            .find(|record| record.invocation_id == invocation_id)
    }

    pub fn total_recorded_runtime_invocations(&self) -> usize {
        self.runtime_records.values().map(Vec::len).sum()
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
    ///   conflict; no silent overwrite.
    /// - Per-procedure retention is bounded by
    ///   [`PROCEDURE_FEEDBACK_CAPACITY_PER_PROCEDURE`].  Eviction is
    ///   deterministic (lowest `(issued_at, feedback_id)` first).
    pub fn attach_procedure_feedback(
        &mut self,
        feedback: ProcedureFeedback,
    ) -> AndromedaResult<RecordOutcome> {
        if feedback.is_authoritative() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store feedback boundary accepts advisory feedback only",
            ));
        }
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
        now: EngineTimestamp,
    ) -> Vec<ProcedureFeedback> {
        match self.feedback.get(&procedure_id) {
            Some(bucket) => bucket.iter_for_procedure_at(procedure_id, now),
            None => Vec::new(),
        }
    }

    /// Drop expired feedback across every procedure.  Returns the total
    /// number of records removed.
    pub fn prune_expired_procedure_feedback(&mut self, now: EngineTimestamp) -> usize {
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

fn validate_runtime_binding(
    entry: &ProcedureStoreEntry,
    binding: ProcedureContractBinding,
) -> AndromedaResult<()> {
    if entry.binding.procedure_id != binding.procedure_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure store runtime evidence procedure id does not match registered binding",
        ));
    }
    if entry.binding.contract_hash != binding.contract_hash {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure store runtime evidence contract hash does not match registered binding",
        ));
    }
    if entry.binding.catalog_version != binding.catalog_version {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure store runtime evidence catalog version does not match registered binding",
        ));
    }
    if entry.binding.stats_version != binding.stats_version {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure store runtime evidence stats version does not match registered binding",
        ));
    }
    if entry.binding.policy_version != binding.policy_version {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure store runtime evidence policy version does not match registered binding",
        ));
    }
    Ok(())
}

fn validate_decision_binding(
    entry: &ProcedureStoreEntry,
    binding: ProcedureContractBinding,
) -> AndromedaResult<()> {
    if entry.binding.procedure_id != binding.procedure_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure store decision evidence procedure id does not match registered binding",
        ));
    }
    if entry.binding.contract_hash != binding.contract_hash {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure store decision evidence contract hash does not match registered binding",
        ));
    }
    if entry.binding.catalog_version != binding.catalog_version {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure store decision evidence catalog version does not match registered binding",
        ));
    }
    if entry.binding.stats_version != binding.stats_version {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure store decision evidence stats version does not match registered binding",
        ));
    }
    if entry.binding.policy_version != binding.policy_version {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure store decision evidence policy version does not match registered binding",
        ));
    }
    Ok(())
}
