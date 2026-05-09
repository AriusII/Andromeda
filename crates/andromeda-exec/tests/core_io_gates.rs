use andromeda_admission::InvocationRequest;
use andromeda_error::AndromedaErrorKind;
use andromeda_hardware::{PipelineClass, ResourceBudget};
use andromeda_observe::{
    CriticalDecisionKind, EventCorrelation, EventEnvelope, EventId, IoBudgetDecisionTrace,
    IoPipelineStage, IoPlacementDecisionTrace, IoStorageTier, ProtocolCorrelation, TraceEvent,
    TraceId,
};
use andromeda_procedure_contract::{
    PolicyVersion, ProcedureContractBinding, ProcedureContractRef, StatsVersion,
};
use andromeda_result_stream::CompletionStatus;
use andromeda_storage_page::PageSize;
use andromeda_storage_placement::{
    CoreIoPlacementPolicy, CoreIoPlacementRequest, OperationalProfile, StorageIoBudgetScope,
    StorageTier, StorageWorkloadClass,
};
use andromeda_types::{
    CatalogVersion, ContractHash, InvocationId, ProcedureId, RequestId, SessionId,
};

const EXEC_CRITICAL_PATH_SOURCES: &[(&str, &str)] = &[
    ("local_runtime", include_str!("../src/local.rs")),
    ("local_dispatch", include_str!("../src/dispatch/local.rs")),
    (
        "invocation_wal",
        include_str!("../../andromeda-wal/src/invocation_wal.rs"),
    ),
    (
        "admission",
        include_str!("../../andromeda-admission/src/service.rs"),
    ),
    (
        "pre_transaction",
        include_str!("../../andromeda-admission/src/pre_transaction.rs"),
    ),
    (
        "completion",
        include_str!("../../andromeda-execution-trace/src/completion.rs"),
    ),
];

#[test]
fn exec_critical_path_has_no_gpu_sql_grpc_runtime_json_or_unsafe_surface() {
    for (name, source) in EXEC_CRITICAL_PATH_SOURCES {
        let normalized = source.to_ascii_lowercase();

        for forbidden in [
            "gpu",
            "grpc",
            "tonic",
            "serde_json",
            "runtime json",
            "select *",
            "unsafe",
        ] {
            assert!(
                !contains_forbidden_token(&normalized, forbidden),
                "{name} critical path must not contain forbidden token {forbidden:?}"
            );
        }
    }
}

#[test]
fn admission_to_core_io_hotstore_plan_emits_observable_budget_and_placement_evidence() {
    let request = invocation_request();
    let admission = request
        .validate_admission(TraceId::new(100))
        .expect("valid invocation contract identity must be admitted before core IO planning");
    assert_eq!(admission.decision, CriticalDecisionKind::ResourceGovernance);
    assert!(admission.has_explanation());

    let correlation = request_correlation();
    let admission_envelope = EventEnvelope::new(
        EventId::new(101),
        correlation,
        TraceEvent::Decision(admission),
    )
    .expect("admission decision must be accepted as observability evidence");
    assert!(admission_envelope.correlation.has_request_session());
    assert!(admission_envelope.correlation.has_contract_catalog());
    assert!(admission_envelope.correlation.has_no_transaction_evidence());

    let profile = OperationalProfile::hot_write();
    profile
        .validate()
        .expect("hot-write operational profile must validate before IO admission");
    let policy = CoreIoPlacementPolicy::new(profile.hardware.clone(), profile.workflow.thresholds);
    let plan = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::HotAppend,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            profile.workflow.page_budget.path_budget,
            false,
        ))
        .expect("hot append should be admitted to HotStore without GPU");

    assert_eq!(plan.pipeline_class, PipelineClass::ForegroundExecution);
    assert_eq!(plan.placement.target_tier, StorageTier::HotStore);
    assert!(plan.placement.mutation_allowed);
    assert!(!plan.gpu_enabled);

    let placement_envelope = EventEnvelope::new(
        EventId::new(102),
        correlation,
        TraceEvent::IoPlacementDecision(
            IoPlacementDecisionTrace::accepted(
                TraceId::new(103),
                plan.pipeline_class,
                IoPipelineStage::Hot,
                IoStorageTier::Hot,
                plan.placement.reason,
            )
            .expect("placement helper requires explicit reason evidence"),
        ),
    )
    .expect("placement plan must carry typed tier, stage, pipeline, and reason evidence");
    assert_eq!(
        placement_envelope.event.kind(),
        CriticalDecisionKind::IoPlacementDecision
    );

    let budget_envelope = EventEnvelope::new(
        EventId::new(104),
        correlation,
        TraceEvent::IoBudgetDecision(
            IoBudgetDecisionTrace::from_budget_request(
                TraceId::new(105),
                plan.pipeline_class,
                IoPipelineStage::Hot,
                ResourceBudget::new(PageSize::KiB16.bytes() as u64, 1, 1),
                PageSize::KiB16.bytes() as u64,
                0,
                1,
                "hot append page request fits declared core IO budget",
            )
            .expect("budget helper requires explicit reason evidence"),
        ),
    )
    .expect("admitted hot append must carry explicit budget evidence");
    assert_eq!(
        budget_envelope.event.kind(),
        CriticalDecisionKind::IoBudgetValidation
    );
}

#[test]
fn admission_rejection_remains_pre_transaction_and_blocks_core_io_planning() {
    let mut request = invocation_request();
    request.invocation_id = InvocationId::new(0);

    let reject = request
        .validate_admission(TraceId::new(200))
        .expect_err("zero invocation ids must be rejected before core IO planning");
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
    assert!(reject.reason.contains("InvocationId"));

    let trace = reject
        .contract_rejected_trace(TraceId::new(201), ProtocolCorrelation::empty(), 1, 2)
        .expect("contract rejection must be traceable");
    let envelope = EventEnvelope::new(
        EventId::new(202),
        request_correlation(),
        TraceEvent::ContractRejected(trace),
    )
    .expect("pre-transaction rejection evidence must not require transaction correlation");

    assert_eq!(
        envelope.event.kind(),
        CriticalDecisionKind::ContractRejected
    );
    assert!(envelope.correlation.has_no_transaction_evidence());
}

#[test]
fn pre_transaction_contract_rejection_blocks_runtime_io_admission_evidence() {
    let request = invocation_request();
    let mut executable_contract = request.procedure;
    executable_contract.contract_hash = ContractHash::test_vector(99);

    let reject = request
        .validate_before_transaction(test_binding(executable_contract), TraceId::new(300))
        .expect_err("contract hash mismatch must be rejected before transaction or IO planning");
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
    assert!(reject.reason.contains("ContractHash"));

    let trace = reject
        .contract_rejected_trace(TraceId::new(301), ProtocolCorrelation::empty(), 1, 3)
        .expect("pre-transaction contract rejection must produce audit evidence");
    let envelope = EventEnvelope::new(
        EventId::new(302),
        request_correlation(),
        TraceEvent::ContractRejected(trace),
    )
    .expect("contract rejection evidence must not require transaction or WAL correlation");

    assert_eq!(
        envelope.event.kind(),
        CriticalDecisionKind::ContractRejected
    );
    assert!(envelope.correlation.has_request_session());
    assert!(envelope.correlation.has_contract_catalog());
    assert!(envelope.correlation.has_no_transaction_evidence());
}

#[test]
fn runtime_io_admission_rejects_gpu_on_wal_append_even_when_gpu_exists() {
    let profile = OperationalProfile::analytics_off_critical_path();
    profile
        .validate()
        .expect("analytics profile may declare GPU only for off-critical-path work");
    let policy = CoreIoPlacementPolicy::new(profile.hardware.clone(), profile.workflow.thresholds);

    let rejection = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::WalAppend,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            profile.workflow.page_budget.path_budget,
            true,
        ))
        .expect_err("WAL append must never be admitted with GPU execution");

    assert_eq!(rejection.kind(), AndromedaErrorKind::Resource);
    assert!(rejection.message().contains("GPU"));
}

#[test]
fn doctrine_scan_helper_uses_token_boundaries_without_weakening_forbidden_list() {
    assert!(contains_forbidden_token("call unsafe now", "unsafe"));
    assert!(contains_forbidden_token("serde_json::value", "serde_json"));
    assert!(contains_forbidden_token("select * from t", "select *"));

    assert!(!contains_forbidden_token("safely_unsafe_name", "unsafe"));
    assert!(!contains_forbidden_token("grouped by capability", "gpu"));
    assert!(!contains_forbidden_token("tonicity", "tonic"));
}

fn contains_forbidden_token(source: &str, forbidden: &str) -> bool {
    if forbidden.contains(' ') || forbidden.contains('*') || forbidden.contains('_') {
        return source.contains(forbidden);
    }

    source
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == forbidden)
}

fn invocation_request() -> InvocationRequest {
    let procedure = ProcedureContractRef {
        procedure_id: ProcedureId::new(2),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(3),
    };
    InvocationRequest {
        invocation_id: InvocationId::new(1),
        procedure,
        expected_binding: Some(test_binding(procedure)),
        expected_contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(3),
        structured_parameters: Vec::new(),
    }
}

fn test_binding(procedure: ProcedureContractRef) -> ProcedureContractBinding {
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
