use andromeda_catalog::ProcedureContractRef;
use andromeda_core::{
    AndromedaResult, CatalogVersion, ContractHash, InvocationId, RequestId, SessionId,
};
use andromeda_observe::{
    AuthorizationDeniedTrace, ContractRejectedTrace, DecisionTrace, ExecutionTransitionTrace,
    ProtocolCorrelation, TraceId, TransitionReasonCode,
};
use andromeda_proto::StructuredObjectHeader;

use crate::{
    CompletionStatus, InvocationCompletion,
    services::{AdmissionService, PreTransactionValidationService},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationRequest {
    pub invocation_id: InvocationId,
    pub procedure: ProcedureContractRef,
    pub expected_contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub structured_parameters: Vec<StructuredObjectHeader>,
}

impl InvocationRequest {
    pub fn validate_admission(&self, trace_id: TraceId) -> Result<DecisionTrace, InvocationReject> {
        AdmissionService::validate_invocation_request(self, trace_id)
    }

    pub fn validate_before_transaction(
        &self,
        executable_contract: ProcedureContractRef,
        trace_id: TraceId,
    ) -> Result<DecisionTrace, InvocationReject> {
        PreTransactionValidationService::validate_invocation_contract(
            self,
            executable_contract,
            trace_id,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationReject {
    pub status: CompletionStatus,
    pub reason: String,
}

impl InvocationReject {
    pub fn authorization_denial_trace(
        &self,
        trace_id: TraceId,
        denied_permission: impl Into<String>,
    ) -> Option<AuthorizationDeniedTrace> {
        (self.status == CompletionStatus::PermissionDenied).then(|| AuthorizationDeniedTrace {
            trace_id,
            denied_permission: denied_permission.into(),
            reason: self.reason.clone(),
        })
    }

    pub fn contract_rejected_trace(
        &self,
        trace_id: TraceId,
        protocol: ProtocolCorrelation,
        contract_kind: u16,
        rejection_code: u16,
    ) -> Option<ContractRejectedTrace> {
        (self.status == CompletionStatus::ContractRejected).then(|| ContractRejectedTrace {
            trace_id,
            protocol,
            contract_kind: Some(contract_kind),
            rejection_code: Some(rejection_code),
            reason: self.reason.clone(),
        })
    }

    /// Project a pre-transaction rejection into an
    /// [`ExecutionTransitionTrace`] so admission/contract rejections produce
    /// the same shape of audit evidence as terminal completions. Pre-
    /// transaction rejections must never carry transaction or durable LSN
    /// evidence (no transaction was ever created), and `validate()` enforces
    /// that contract.
    pub fn project_transition(
        &self,
        invocation_id: InvocationId,
        trace_id: TraceId,
        request_id: Option<RequestId>,
        session_id: Option<SessionId>,
    ) -> ExecutionTransitionTrace {
        let reason_code = match self.status {
            CompletionStatus::PermissionDenied => TransitionReasonCode::PERMISSION_DENIED,
            CompletionStatus::ContractRejected | CompletionStatus::FailedBeforeTransaction => {
                TransitionReasonCode::PRE_TRANSACTION_REJECTION
            }
            CompletionStatus::Cancelled => TransitionReasonCode::CANCELLED,
            CompletionStatus::SystemUnavailable => TransitionReasonCode::SYSTEM_UNAVAILABLE,
            CompletionStatus::Poisoned => TransitionReasonCode::POISON,
            CompletionStatus::Committed | CompletionStatus::RolledBack => {
                // A reject that claims a transactional terminal status is
                // a misuse upstream; we fall back to a non-terminal reason
                // so the trace cannot fabricate a durable claim.
                TransitionReasonCode::EXECUTOR_FAILURE
            }
        };
        ExecutionTransitionTrace {
            trace_id,
            invocation_id,
            request_id,
            session_id,
            transaction_id: None,
            completion_code: Some(self.status.terminal_code()),
            prev_phase: None,
            next_phase: None,
            durable_lsn: None,
            reason_code,
            reason: self.reason.clone(),
        }
    }
}

pub trait ProcedureInvoker {
    fn invoke(&self, request: InvocationRequest) -> AndromedaResult<InvocationCompletion>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{CatalogVersion, ProcedureId, RequestId, SessionId};
    use andromeda_observe::{EventCorrelation, EventEnvelope, EventId, TraceEvent};

    fn request(expected_contract_hash: ContractHash) -> InvocationRequest {
        InvocationRequest {
            invocation_id: InvocationId::new(1),
            procedure: ProcedureContractRef {
                procedure_id: ProcedureId::new(2),
                contract_hash: ContractHash::test_vector(7),
                catalog_version: CatalogVersion::new(3),
            },
            expected_contract_hash,
            catalog_version: CatalogVersion::new(3),
            structured_parameters: Vec::new(),
        }
    }

    fn request_correlation() -> EventCorrelation {
        EventCorrelation {
            request_id: Some(RequestId::new(10)),
            session_id: Some(SessionId::new(11)),
            contract_hash: Some(ContractHash::test_vector(7)),
            catalog_version: Some(CatalogVersion::new(3)),
            catalog_object_id: None,
            transaction_id: None,
            durable_lsn: None,
            protocol: None,
        }
    }

    #[test]
    fn invocation_rejects_contract_mismatch_before_transaction_creation() {
        let reject = request(ContractHash::test_vector(8))
            .validate_before_transaction(
                request(ContractHash::test_vector(7)).procedure,
                TraceId::new(1),
            )
            .unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
    }

    #[test]
    fn invocation_rejects_zero_id_during_admission_before_transaction_creation() {
        let mut request = request(ContractHash::test_vector(7));
        request.invocation_id = InvocationId::new(0);

        let reject = request.validate_admission(TraceId::new(1)).unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
        assert!(reject.reason.contains("InvocationId"));
    }

    #[test]
    fn invocation_accepts_contract_before_transaction_creation() {
        let trace = request(ContractHash::test_vector(7))
            .validate_before_transaction(
                request(ContractHash::test_vector(7)).procedure,
                TraceId::new(1),
            )
            .unwrap();

        assert!(trace.has_explanation());
    }

    #[test]
    fn invocation_rejects_procedure_id_mismatch_before_transaction_creation() {
        let mut executable = request(ContractHash::test_vector(7)).procedure;
        executable.procedure_id = ProcedureId::new(99);

        let reject = request(ContractHash::test_vector(7))
            .validate_before_transaction(executable, TraceId::new(1))
            .unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
        assert!(reject.reason.contains("ProcedureId"));
    }

    #[test]
    fn invocation_reject_can_be_emitted_as_pre_transaction_audit_evidence() {
        let reject = request(ContractHash::test_vector(8))
            .validate_before_transaction(
                request(ContractHash::test_vector(7)).procedure,
                TraceId::new(1),
            )
            .unwrap_err();
        let trace = reject
            .contract_rejected_trace(TraceId::new(55), ProtocolCorrelation::empty(), 1, 2)
            .expect("contract reject produces contract evidence trace");

        let envelope = EventEnvelope::new(
            EventId::new(56),
            request_correlation(),
            TraceEvent::ContractRejected(trace),
        )
        .expect("contract rejection is auditable without transaction evidence");

        assert!(envelope.correlation.has_contract_catalog());
        assert!(envelope.correlation.has_no_transaction_evidence());
    }
}
