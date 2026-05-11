//! GAP-02 regression: `ProcedureRegistry::dispatch()` requires a pre-dispatch
//! `ProcedureContractBinding` and rejects mismatched bindings before the caller
//! can inspect the handler result.
//!
//! Invariant: NO code path can reach handler result propagation without a
//! caller-presented, cross-checked `ProcedureContractBinding`.

use andromeda_admission::InvocationContext;
use andromeda_error::AndromedaErrorKind;
use andromeda_execution::{LocalProcedure, ProcedureHandler, ProcedureRegistry};
use andromeda_observability::TraceId;
use andromeda_procedure_contract::{
    PolicyVersion, ProcedureContractBinding, ProcedureContractRef, StatsVersion,
};
use andromeda_result_stream::ResultStreamMetadata;
use andromeda_srpl_ir::Cardinality;
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

fn contract(id: u64) -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: ProcedureId::new(id),
        contract_hash: ContractHash::test_vector(id as u8),
        catalog_version: CatalogVersion::new(3),
    }
}

fn binding(contract: ProcedureContractRef) -> ProcedureContractBinding {
    ProcedureContractBinding {
        procedure_id: contract.procedure_id,
        catalog_version: contract.catalog_version,
        contract_hash: contract.contract_hash,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([contract.procedure_id.get() as u8; PolicyVersion::LEN]),
    }
}

fn context() -> InvocationContext {
    InvocationContext::new(
        TraceId::new(900),
        vec!["procedure.test.execute".to_string()],
    )
}

// A minimal handler that always returns a LocalProcedure with its own contract.
#[derive(Clone, Copy)]
struct MinimalHandler {
    contract: ProcedureContractRef,
}

impl ProcedureHandler for MinimalHandler {
    fn procedure_id(&self) -> ProcedureId {
        self.contract.procedure_id
    }

    fn contract(&self) -> ProcedureContractRef {
        self.contract
    }

    fn result_metadata(&self) -> ResultStreamMetadata {
        ResultStreamMetadata::exact(1, 1, Cardinality::One, 1)
    }

    fn execute(
        &self,
        _context: InvocationContext,
    ) -> andromeda_error::AndromedaResult<LocalProcedure> {
        Ok(LocalProcedure {
            contract: self.contract,
            contract_binding: binding(self.contract),
            required_permissions: vec!["procedure.test.execute".to_string()],
            result_metadata: self.result_metadata(),
            mutation_payload: vec![1],
            rows_affected: 1,
        })
    }
}

/// GAP-02 regression: `dispatch()` with the correct binding for the registered
/// procedure succeeds.
#[test]
fn dispatch_with_correct_binding_succeeds() {
    let c = contract(1001);
    let mut registry = ProcedureRegistry::new();
    registry.register(MinimalHandler { contract: c }).unwrap();

    let procedure = registry
        .dispatch(c.procedure_id, binding(c), context())
        .expect("dispatch with correct binding must succeed");

    assert_eq!(procedure.contract.procedure_id, c.procedure_id);
    assert_eq!(procedure.contract_binding, binding(c));
}

/// GAP-02 regression: `dispatch()` without a pre-dispatch binding (i.e. the
/// caller presents a binding whose `contract_hash` differs from the handler's
/// result) MUST be rejected with a `Contract` error before the result is
/// returned to the caller.
///
/// This proves that no code path can reach handler result propagation without
/// a checked `ProcedureContractBinding`.
#[test]
fn dispatch_without_binding_rejected() {
    let c = contract(1002);
    let mut registry = ProcedureRegistry::new();
    registry.register(MinimalHandler { contract: c }).unwrap();

    // Construct a binding whose `contract_hash` is intentionally wrong — this
    // simulates a caller that presents a stale or fabricated binding token.
    let wrong_binding = ProcedureContractBinding {
        procedure_id: c.procedure_id,
        catalog_version: c.catalog_version,
        contract_hash: ContractHash::test_vector(0xFF), // wrong hash
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([0xAB; PolicyVersion::LEN]),
    };

    let error = registry
        .dispatch(c.procedure_id, wrong_binding, context())
        .expect_err("dispatch with mismatched binding must be rejected");

    assert_eq!(
        error.kind(),
        AndromedaErrorKind::Contract,
        "binding mismatch must produce a Contract-kind error, got: {:?}",
        error.kind()
    );
    assert!(
        error.message().contains("ProcedureContractBinding"),
        "error must mention ProcedureContractBinding, got: {}",
        error.message()
    );
}

/// GAP-02 regression: `dispatch()` for an unknown procedure returns an
/// `Execution`-kind error (no binding check is reached since the handler
/// lookup fails first).
#[test]
fn dispatch_unknown_procedure_returns_execution_error() {
    let registry = ProcedureRegistry::new();
    let unknown = contract(9999);

    let error = registry
        .dispatch(unknown.procedure_id, binding(unknown), context())
        .expect_err("dispatch for unknown procedure must fail");

    assert_eq!(
        error.kind(),
        AndromedaErrorKind::Execution,
        "unknown procedure must produce an Execution-kind error"
    );
}

/// GAP-02 regression: `dispatch()` with a binding whose `stats_version` or
/// `policy_version` differ from the handler result is also rejected.
#[test]
fn dispatch_with_stats_version_drift_rejected() {
    let c = contract(1003);
    let mut registry = ProcedureRegistry::new();
    registry.register(MinimalHandler { contract: c }).unwrap();

    // The handler produces binding with StatsVersion::new(1). Presenter
    // gives a different stats_version, which must be rejected.
    let drifted_binding = ProcedureContractBinding {
        procedure_id: c.procedure_id,
        catalog_version: c.catalog_version,
        contract_hash: c.contract_hash,
        stats_version: StatsVersion::new(99), // drifted
        policy_version: PolicyVersion::new([c.procedure_id.get() as u8; PolicyVersion::LEN]),
    };

    let error = registry
        .dispatch(c.procedure_id, drifted_binding, context())
        .expect_err("dispatch with drifted stats_version must be rejected");

    assert_eq!(
        error.kind(),
        AndromedaErrorKind::Contract,
        "stats_version drift must produce a Contract-kind error"
    );
}
