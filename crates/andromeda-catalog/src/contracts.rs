//! Temporary compatibility facade for Procedure contracts.
//!
//! Lot 2.1 moves the canonical contract model to `andromeda-procedure-contract`.
//! `andromeda-catalog` keeps these explicit reexports for one migration cycle
//! so existing imports such as `andromeda_catalog::ProcedureContract` remain
//! valid while call sites migrate deliberately.

pub use andromeda_procedure_contract::{
    AccessMode, CompatibilityPolicy, ContractCompatibilityDiagnostic, IsolationPolicy,
    MultiResultPolicy, PolicyVersion, ProcedureContract, ProcedureContractBinding,
    ProcedureContractCandidate, ProcedureContractRef, ProcedureErrorPolicy, ProtocolLayoutRef,
    ResultMetadataPolicy, ResultStreamCardinality, ResultStreamContract, StatsVersion,
    TransactionPolicy, diagnose_procedure_contract_compatibility,
};
