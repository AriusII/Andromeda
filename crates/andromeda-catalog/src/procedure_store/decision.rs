use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash,
    InvocationId, ProcedureId,
};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

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
