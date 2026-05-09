use andromeda_digest::Sha256;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::TraceId;
use andromeda_plan_cache::{PlanCacheKey, PlanClass};
use andromeda_procedure_contract::{PolicyVersion, ProcedureContractBinding, StatsVersion};
use andromeda_time::EngineTimestamp;
use andromeda_types::{CatalogVersion, ContractHash, InvocationId, ProcedureId};

use crate::{
    ProcedureRuntimeCounters, ProcedureRuntimePlanId, ProcedureRuntimeRecordId,
    ProcedureRuntimeStatus, ProcedureStoreEvidenceRole,
};

const PROCEDURE_RUNTIME_RECORD_DOMAIN: &[u8] = b"andromeda.procedure_store.runtime-record.v1";

/// Outcome of storing a terminal invocation runtime record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationRuntimeRecordOutcome {
    Stored,
    Duplicate,
}

/// Terminal runtime evidence for one Procedure invocation.
///
/// The record binds observed runtime counters to the complete
/// [`ProcedureContractBinding`] tuple. Consumers can reject stale records when
/// any contract, catalog, statistics, or policy identity drifts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationRuntimeRecord {
    pub record_id: ProcedureRuntimeRecordId,
    pub invocation_id: InvocationId,
    pub binding: ProcedureContractBinding,
    pub started_at: EngineTimestamp,
    pub completed_at: EngineTimestamp,
    pub duration_millis: u64,
    pub decision_trace_id: Option<TraceId>,
    pub plan_key: Option<PlanCacheKey>,
    pub plan_id: Option<ProcedureRuntimePlanId>,
    pub(crate) evidence_role: ProcedureStoreEvidenceRole,
    pub counters: ProcedureRuntimeCounters,
    pub status: ProcedureRuntimeStatus,
    pub error_kind: Option<AndromedaErrorKind>,
}

impl InvocationRuntimeRecord {
    #[allow(
        clippy::too_many_arguments,
        reason = "Runtime evidence construction keeps every audited input explicit."
    )]
    pub fn new(
        invocation_id: InvocationId,
        binding: ProcedureContractBinding,
        started_at: EngineTimestamp,
        completed_at: EngineTimestamp,
        plan_key: Option<PlanCacheKey>,
        plan_id: Option<ProcedureRuntimePlanId>,
        counters: ProcedureRuntimeCounters,
        status: ProcedureRuntimeStatus,
        error_kind: Option<AndromedaErrorKind>,
    ) -> AndromedaResult<Self> {
        Self::new_with_optional_decision_trace(
            invocation_id,
            binding,
            started_at,
            completed_at,
            None,
            plan_key,
            plan_id,
            counters,
            status,
            error_kind,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Runtime evidence construction keeps every audited input explicit."
    )]
    pub fn new_with_decision_trace(
        invocation_id: InvocationId,
        binding: ProcedureContractBinding,
        started_at: EngineTimestamp,
        completed_at: EngineTimestamp,
        decision_trace_id: TraceId,
        plan_key: Option<PlanCacheKey>,
        plan_id: Option<ProcedureRuntimePlanId>,
        counters: ProcedureRuntimeCounters,
        status: ProcedureRuntimeStatus,
        error_kind: Option<AndromedaErrorKind>,
    ) -> AndromedaResult<Self> {
        Self::new_with_optional_decision_trace(
            invocation_id,
            binding,
            started_at,
            completed_at,
            Some(decision_trace_id),
            plan_key,
            plan_id,
            counters,
            status,
            error_kind,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Internal constructor keeps every audited input explicit."
    )]
    fn new_with_optional_decision_trace(
        invocation_id: InvocationId,
        binding: ProcedureContractBinding,
        started_at: EngineTimestamp,
        completed_at: EngineTimestamp,
        decision_trace_id: Option<TraceId>,
        plan_key: Option<PlanCacheKey>,
        plan_id: Option<ProcedureRuntimePlanId>,
        counters: ProcedureRuntimeCounters,
        status: ProcedureRuntimeStatus,
        error_kind: Option<AndromedaErrorKind>,
    ) -> AndromedaResult<Self> {
        let duration_millis = completed_at
            .as_unix_millis()
            .checked_sub(started_at.as_unix_millis())
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "invocation runtime completed_at must not precede started_at",
                )
            })?;

        let mut record = Self {
            record_id: ProcedureRuntimeRecordId::zero(),
            invocation_id,
            binding,
            started_at,
            completed_at,
            duration_millis,
            decision_trace_id,
            plan_key,
            plan_id,
            evidence_role: ProcedureStoreEvidenceRole::OBSERVED_FEEDBACK,
            counters,
            status,
            error_kind,
        };
        record.record_id = ProcedureRuntimeRecordId::from_record(&record);
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.invocation_id.get() == 0 {
            return Err(runtime_contract_error(
                "invocation runtime record invocation id must not be zero",
            ));
        }
        self.binding.validate()?;
        if self.started_at.is_zero() {
            return Err(runtime_contract_error(
                "invocation runtime started_at must not be zero",
            ));
        }
        if self.completed_at.is_zero() {
            return Err(runtime_contract_error(
                "invocation runtime completed_at must not be zero",
            ));
        }
        if self.record_id.is_zero() {
            return Err(runtime_contract_error(
                "invocation runtime record id must not be zero",
            ));
        }
        if let Some(decision_trace_id) = self.decision_trace_id
            && decision_trace_id.is_zero()
        {
            return Err(runtime_contract_error(
                "invocation runtime decision trace id must not be zero when present",
            ));
        }
        if !self.evidence_role.is_observed_feedback() {
            return Err(runtime_contract_error(
                "invocation runtime evidence role must remain observed feedback",
            ));
        }
        let expected_duration = self
            .completed_at
            .as_unix_millis()
            .checked_sub(self.started_at.as_unix_millis())
            .ok_or_else(|| {
                runtime_contract_error(
                    "invocation runtime completed_at must not precede started_at",
                )
            })?;
        if expected_duration != self.duration_millis {
            return Err(runtime_contract_error(
                "invocation runtime duration must equal completed_at - started_at",
            ));
        }
        self.validate_plan_binding()?;
        if self.status == ProcedureRuntimeStatus::Committed && self.error_kind.is_some() {
            return Err(runtime_contract_error(
                "committed invocation runtime record must not carry an error kind",
            ));
        }
        if self.status.requires_error_kind() && self.error_kind.is_none() {
            return Err(runtime_contract_error(
                "non-committed invocation runtime record must carry an error kind",
            ));
        }
        if self.status == ProcedureRuntimeStatus::Aborted && !self.counters.is_empty() {
            return Err(runtime_contract_error(
                "aborted pre-transaction invocation runtime record must not carry data or WAL counters",
            ));
        }
        if self.counters.has_rows_written() && self.counters.wal_bytes == 0 {
            return Err(runtime_contract_error(
                "write invocation runtime record requires non-zero WAL bytes",
            ));
        }
        if self.counters.has_temp_or_spill_bytes() && self.plan_key.is_none() {
            return Err(runtime_contract_error(
                "invocation runtime temp or spill bytes require a recorded plan cache key",
            ));
        }
        if self.record_id != ProcedureRuntimeRecordId::from_record(self) {
            return Err(runtime_contract_error(
                "invocation runtime record id must match deterministic runtime evidence digest",
            ));
        }
        Ok(())
    }

    pub const fn procedure_id(&self) -> ProcedureId {
        self.binding.procedure_id
    }

    pub const fn binding(&self) -> ProcedureContractBinding {
        self.binding
    }

    pub const fn expected_contract_hash(&self) -> ContractHash {
        self.binding.contract_hash
    }

    pub const fn expected_catalog_version(&self) -> CatalogVersion {
        self.binding.catalog_version
    }

    pub const fn expected_stats_version(&self) -> StatsVersion {
        self.binding.stats_version
    }

    pub const fn expected_policy_version(&self) -> PolicyVersion {
        self.binding.policy_version
    }

    pub const fn plan_class(&self) -> Option<PlanClass> {
        match self.plan_key {
            Some(plan_key) => Some(plan_key.plan_class),
            None => None,
        }
    }

    pub const fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    pub const fn is_authoritative_decision(&self) -> bool {
        self.evidence_role.is_authoritative_decision()
    }

    pub const fn is_observed_feedback(&self) -> bool {
        self.evidence_role.is_observed_feedback()
    }

    pub const fn evidence_role(&self) -> ProcedureStoreEvidenceRole {
        self.evidence_role
    }

    pub const fn can_select_plan_alone(&self) -> bool {
        self.evidence_role.can_select_plan_alone()
    }

    fn validate_plan_binding(&self) -> AndromedaResult<()> {
        match (self.plan_key, self.plan_id) {
            (None, None) => Ok(()),
            (None, Some(_)) => Err(runtime_contract_error(
                "invocation runtime plan id requires a recorded plan cache key",
            )),
            (Some(_), None) => Err(runtime_contract_error(
                "invocation runtime plan cache key requires a recorded plan id",
            )),
            (Some(plan_key), Some(plan_id)) => {
                if plan_id.is_zero() {
                    return Err(runtime_contract_error(
                        "invocation runtime plan id must not be zero when present",
                    ));
                }
                if plan_id != ProcedureRuntimePlanId::from_plan_cache_key(&plan_key) {
                    return Err(runtime_contract_error(
                        "invocation runtime plan id must match the recorded plan cache key digest",
                    ));
                }
                if plan_key.procedure_id != self.binding.procedure_id {
                    return Err(runtime_contract_error(
                        "invocation runtime plan cache key procedure id must match binding",
                    ));
                }
                if plan_key.contract_hash != self.binding.contract_hash {
                    return Err(runtime_contract_error(
                        "invocation runtime plan cache key contract hash must match binding",
                    ));
                }
                if plan_key.catalog_version != self.binding.catalog_version {
                    return Err(runtime_contract_error(
                        "invocation runtime plan cache key catalog version must match binding",
                    ));
                }
                if plan_key.stats_version != self.binding.stats_version {
                    return Err(runtime_contract_error(
                        "invocation runtime plan cache key stats version must match binding",
                    ));
                }
                if plan_key.policy_version != self.binding.policy_version {
                    return Err(runtime_contract_error(
                        "invocation runtime plan cache key policy version must match binding",
                    ));
                }
                Ok(())
            },
        }
    }
}

pub(crate) fn compute_runtime_record_id(
    record: &InvocationRuntimeRecord,
) -> [u8; ProcedureRuntimeRecordId::LEN] {
    let mut hasher = Sha256::new();
    hasher.update(PROCEDURE_RUNTIME_RECORD_DOMAIN);

    hasher.update(&[0xA0]);
    hasher.update(&record.invocation_id.get().to_le_bytes());

    hasher.update(&[0xA1]);
    hasher.update(&record.binding.procedure_id.get().to_le_bytes());
    hasher.update(&[0xA2]);
    hasher.update(&record.binding.contract_hash.as_bytes());
    hasher.update(&[0xA3]);
    hasher.update(&record.binding.catalog_version.get().to_le_bytes());
    hasher.update(&[0xA4]);
    hasher.update(&record.binding.stats_version.get().to_le_bytes());
    hasher.update(&[0xA5]);
    hasher.update(&record.binding.policy_version.as_bytes());

    hasher.update(&[0xA6]);
    hasher.update(&record.started_at.as_unix_millis().to_le_bytes());
    hasher.update(&[0xA7]);
    hasher.update(&record.completed_at.as_unix_millis().to_le_bytes());
    hasher.update(&[0xA8]);
    hasher.update(&record.duration_millis.to_le_bytes());

    hasher.update(&[0xA9]);
    match record.decision_trace_id {
        Some(trace_id) => {
            hasher.update(&[0x01]);
            hasher.update(&trace_id.get().to_le_bytes());
        },
        None => {
            hasher.update(&[0x00]);
            hasher.update(&0u128.to_le_bytes());
        },
    }

    hasher.update(&[0xAA]);
    match record.plan_key {
        Some(plan_key) => {
            hasher.update(&[0x01]);
            hasher.update(&plan_key.digest());
        },
        None => {
            hasher.update(&[0x00]);
            hasher.update(&[0u8; 32]);
        },
    }

    hasher.update(&[0xAB]);
    match record.plan_id {
        Some(plan_id) => {
            hasher.update(&[0x01]);
            hasher.update(&plan_id.as_bytes());
        },
        None => {
            hasher.update(&[0x00]);
            hasher.update(&[0u8; 32]);
        },
    }

    hasher.update(&[0xAF, record.evidence_role.as_tag()]);

    hasher.update(&[0xAC]);
    hasher.update(&record.counters.rows_read.to_le_bytes());
    hasher.update(&record.counters.rows_returned.to_le_bytes());
    hasher.update(&record.counters.rows_written.to_le_bytes());
    hasher.update(&record.counters.rows_affected.to_le_bytes());
    hasher.update(&record.counters.logical_reads.to_le_bytes());
    hasher.update(&record.counters.physical_reads.to_le_bytes());
    hasher.update(&record.counters.cache_hits.to_le_bytes());
    hasher.update(&record.counters.wal_bytes.to_le_bytes());
    hasher.update(&record.counters.temp_bytes.to_le_bytes());
    hasher.update(&record.counters.spill_bytes.to_le_bytes());

    hasher.update(&[0xAD, record.status.as_tag()]);
    hasher.update(&[0xAE]);
    match record.error_kind {
        Some(kind) => {
            hasher.update(&[0x01, error_kind_tag(kind)]);
        },
        None => hasher.update(&[0x00, 0x00]),
    }

    hasher.finalize()
}

const fn error_kind_tag(kind: AndromedaErrorKind) -> u8 {
    match kind {
        AndromedaErrorKind::Catalog => 0x01,
        AndromedaErrorKind::Contract => 0x02,
        AndromedaErrorKind::Execution => 0x03,
        AndromedaErrorKind::Internal => 0x04,
        AndromedaErrorKind::Protocol => 0x05,
        AndromedaErrorKind::Resource => 0x06,
        AndromedaErrorKind::Security => 0x07,
        AndromedaErrorKind::Srpl => 0x08,
        AndromedaErrorKind::Storage => 0x09,
        AndromedaErrorKind::Timeout => 0x0A,
        AndromedaErrorKind::Transaction => 0x0B,
        AndromedaErrorKind::Transport => 0x0C,
    }
}

fn runtime_contract_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_procedure_contract::{PolicyVersion, StatsVersion};
    use andromeda_time::EngineTimestamp;
    use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

    fn binding() -> ProcedureContractBinding {
        ProcedureContractBinding {
            procedure_id: ProcedureId::new(1),
            catalog_version: CatalogVersion::new(7),
            contract_hash: ContractHash::test_vector(42),
            stats_version: StatsVersion::new(1),
            policy_version: PolicyVersion::new([1; PolicyVersion::LEN]),
        }
    }

    #[test]
    fn runtime_record_rejects_internal_authoritative_role_tampering() {
        let mut record = InvocationRuntimeRecord::new(
            InvocationId::new(1),
            binding(),
            EngineTimestamp::from_unix_millis(1_000),
            EngineTimestamp::from_unix_millis(1_001),
            None,
            None,
            ProcedureRuntimeCounters::new(1, 0, 0, 0),
            ProcedureRuntimeStatus::Committed,
            None,
        )
        .expect("valid runtime record");
        record.evidence_role = ProcedureStoreEvidenceRole::AUTHORITATIVE_DECISION;

        let err = record.validate().unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("observed feedback"));
    }
}
