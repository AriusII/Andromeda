use andromeda_catalog::{ProcedureContractBinding, ProcedureContractRef};
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
    services::{AdmissionService, PreTransactionValidationService, ProcedureBindingEvidence},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationRequest {
    pub invocation_id: InvocationId,
    pub procedure: ProcedureContractRef,
    pub expected_binding: Option<ProcedureContractBinding>,
    pub expected_contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub structured_parameters: Vec<StructuredObjectHeader>,
}

impl InvocationRequest {
    /// Validate the invocation request at the admission gate before a transaction is created.
    ///
    /// The current implementation enforces structural invariants that can be checked
    /// without resource or permission context: the `InvocationId` must be nonzero so
    /// that every invocation carries an observable, auditable identity.
    ///
    /// TECH-DEBT: Full admission control — resource budgets, rate limits, and execution
    /// IO placement decisions — belongs to `ExecutionIoAdmissionRequest::validate_admission`
    /// in `admission.rs`. The two paths should be composed into a unified pre-transaction
    /// gate once the admission service is wired to the runtime dispatcher. Risk: until
    /// then, callers that bypass the IO admission path receive only structural checks.
    /// Closure: wire `AdmissionService` into `LocalVerticalRuntime::execute_after_admission`.
    pub fn validate_admission(&self, trace_id: TraceId) -> Result<DecisionTrace, InvocationReject> {
        AdmissionService::validate_invocation_request(self, trace_id)
    }

    pub fn validate_before_transaction(
        &self,
        executable_binding: impl Into<ProcedureBindingEvidence>,
        trace_id: TraceId,
    ) -> Result<DecisionTrace, InvocationReject> {
        PreTransactionValidationService::validate_invocation_contract(
            self,
            executable_binding,
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
                // A reject that claims a transactional terminal status is a misuse upstream;
                // fall back to a non-terminal reason so the trace cannot fabricate a durable
                // claim.
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
        let procedure = ProcedureContractRef {
            procedure_id: ProcedureId::new(2),
            contract_hash: ContractHash::test_vector(7),
            catalog_version: CatalogVersion::new(3),
        };
        InvocationRequest {
            invocation_id: InvocationId::new(1),
            procedure,
            expected_binding: Some(test_binding(procedure)),
            expected_contract_hash,
            catalog_version: CatalogVersion::new(3),
            structured_parameters: Vec::new(),
        }
    }

    fn test_binding(procedure: ProcedureContractRef) -> ProcedureContractBinding {
        use andromeda_catalog::{PolicyVersion, StatsVersion};

        ProcedureContractBinding {
            procedure_id: procedure.procedure_id,
            catalog_version: procedure.catalog_version,
            contract_hash: procedure.contract_hash,
            stats_version: StatsVersion::new(1),
            policy_version: PolicyVersion::new([7; PolicyVersion::LEN]),
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
                test_binding(request(ContractHash::test_vector(7)).procedure),
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
    fn invocation_rejects_missing_expected_binding_during_admission() {
        let mut request = request(ContractHash::test_vector(7));
        request.expected_binding = None;

        let reject = request.validate_admission(TraceId::new(1)).unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
        assert!(reject.reason.contains("ProcedureContractBinding"));
    }

    #[test]
    fn invocation_rejects_legacy_executable_ref_as_missing_binding() {
        let reject = request(ContractHash::test_vector(7))
            .validate_before_transaction(
                request(ContractHash::test_vector(7)).procedure,
                TraceId::new(1),
            )
            .unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
        assert!(reject.reason.contains("ProcedureContractBinding"));
        assert!(reject.reason.contains("ProcedureContractRef"));
    }

    #[test]
    fn missing_binding_reject_can_be_emitted_as_pre_transaction_audit_evidence() {
        let mut request = request(ContractHash::test_vector(7));
        request.expected_binding = None;

        let reject = request.validate_admission(TraceId::new(1)).unwrap_err();
        let trace = reject
            .contract_rejected_trace(TraceId::new(55), ProtocolCorrelation::empty(), 1, 4)
            .expect("missing binding rejection produces contract evidence trace");

        let envelope = EventEnvelope::new(
            EventId::new(57),
            request_correlation(),
            TraceEvent::ContractRejected(trace),
        )
        .expect("missing binding rejection is auditable without transaction evidence");

        assert!(envelope.correlation.has_no_transaction_evidence());
    }

    #[test]
    fn invocation_rejects_stats_version_mismatch_before_transaction_creation() {
        let request = request(ContractHash::test_vector(7));
        let mut executable_binding = request.expected_binding.unwrap();
        executable_binding.stats_version = andromeda_catalog::StatsVersion::new(2);

        let reject = request
            .validate_before_transaction(executable_binding, TraceId::new(1))
            .unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
        assert!(reject.reason.contains("StatsVersion"));
    }

    #[test]
    fn invocation_accepts_contract_before_transaction_creation() {
        let trace = request(ContractHash::test_vector(7))
            .validate_before_transaction(
                test_binding(request(ContractHash::test_vector(7)).procedure),
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
            .validate_before_transaction(test_binding(executable), TraceId::new(1))
            .unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
        assert!(reject.reason.contains("ProcedureId"));
    }

    #[test]
    fn invocation_reject_can_be_emitted_as_pre_transaction_audit_evidence() {
        let reject = request(ContractHash::test_vector(8))
            .validate_before_transaction(
                test_binding(request(ContractHash::test_vector(7)).procedure),
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
