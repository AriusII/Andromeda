use andromeda_error::AndromedaResult;
use andromeda_observe::DecisionTrace;
use andromeda_procedure_store::{
    InvocationRuntimeRecord, InvocationRuntimeRecordOutcome, ProcedureStore,
};
use andromeda_types::TransactionId;

use crate::{InvocationCompletion, ResultStreamMetadata, WalDurabilityEvidence};

pub use andromeda_execution::LocalProcedure;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerticalInvocationOutcome {
    pub completion: InvocationCompletion,
    /// Recovery-safe transaction id allocated by the runtime's
    /// `TransactionManager` for this invocation. This is the authoritative
    /// id stamped on every WAL record and result-stream frame; downstream
    /// emitters must read it from here rather than re-deriving it from
    /// [`InvocationCompletion::invocation_id`].
    pub transaction_id: TransactionId,
    pub admission_trace: DecisionTrace,
    pub contract_trace: DecisionTrace,
    pub authorization_trace: Option<DecisionTrace>,
    pub result_metadata: ResultStreamMetadata,
    pub runtime_record: InvocationRuntimeRecord,
    pub wal_evidence: Option<WalDurabilityEvidence>,
}

impl VerticalInvocationOutcome {
    pub const fn procedure_runtime_record(&self) -> &InvocationRuntimeRecord {
        &self.runtime_record
    }

    pub fn attach_runtime_to_procedure_store(
        &self,
        store: &mut ProcedureStore,
    ) -> AndromedaResult<InvocationRuntimeRecordOutcome> {
        store.attach_invocation_runtime(self.runtime_record.clone())
    }
}
