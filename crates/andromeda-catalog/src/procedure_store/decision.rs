use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_types::{InvocationId, ProcedureId};

use crate::ProcedureContractBinding;

use super::evidence_role::ProcedureStoreEvidenceRole;

/// Decision evidence attached to a single invocation. Every invocation that
/// reaches the store must carry a [`DecisionTrace`] explaining why it was
/// admitted, rejected, contract-validated, etc.
///
/// This struct is the evidence shape; it does not itself decide anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationDecisionRecord {
    pub invocation_id: InvocationId,
    pub binding: ProcedureContractBinding,
    pub trace: DecisionTrace,
}

impl InvocationDecisionRecord {
    pub fn new(
        invocation_id: InvocationId,
        binding: ProcedureContractBinding,
        trace: DecisionTrace,
    ) -> AndromedaResult<Self> {
        let record = Self {
            invocation_id,
            binding,
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
        self.binding.validate()?;
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

    pub const fn procedure_id(&self) -> ProcedureId {
        self.binding.procedure_id
    }

    pub const fn binding(&self) -> ProcedureContractBinding {
        self.binding
    }

    pub fn decision_kind(&self) -> CriticalDecisionKind {
        self.trace.decision
    }

    pub const fn evidence_role(&self) -> ProcedureStoreEvidenceRole {
        ProcedureStoreEvidenceRole::authoritative_decision()
    }

    pub const fn is_authoritative_decision(&self) -> bool {
        self.evidence_role().is_authoritative_decision()
    }

    pub const fn is_observed_feedback(&self) -> bool {
        self.evidence_role().is_observed_feedback()
    }

    pub const fn can_select_plan_alone(&self) -> bool {
        self.evidence_role().can_select_plan_alone()
    }
}
