use andromeda_hardware::{GpuExecutionPolicy, PipelineClass, ResourceBudget};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, InvocationId, ProcedureId, RequestId, SessionId,
};

use super::*;
use crate::{
    AdminOperation, AdminOperationTrace, AuditTrace, AuthorizationDeniedTrace, BackpressureTrace,
    CatalogMutationTrace, CertificateIdentity, CorruptionBoundaryTrace, CriticalDecisionKind,
    DecisionTrace, EventCorrelation, EventEmitter, ExecutionTransitionTrace, InMemoryEventSink,
    IoBudgetDecisionTrace, IoPipelineStage, MvccTrace, Permission, ProtocolCorrelation,
    ProtocolEventScope, RecoveryTrace, SecurityAuditOutcome, SecurityAuditTrace, SurfaceScope,
    TraceEvent, TraceId, TransitionReasonCode, UserPrincipal, UserPrincipalKind, WalEventTrace,
    WalOperation,
};

fn protocol_event(trace_id: u128) -> TraceEvent {
    TraceEvent::Backpressure(BackpressureTrace {
        trace_id: TraceId::new(trace_id),
        scope: ProtocolEventScope::Connection,
        protocol: ProtocolCorrelation::empty(),
        retry_after_micros: Some(10),
        pending_units: None,
        limit_units: None,
        reason: "bounded queue pressure".to_string(),
    })
}

fn wal_event(trace_id: u128, lsn: u64) -> TraceEvent {
    TraceEvent::WalEvent(WalEventTrace {
        trace_id: TraceId::new(trace_id),
        transaction_id: None,
        operation: WalOperation::Append,
        appended_lsn: lsn,
        durable_lsn: None,
    })
}

fn security_event(trace_id: u128, principal: &str) -> TraceEvent {
    TraceEvent::SecurityAudit(
        SecurityAuditTrace::new(
            TraceId::new(trace_id),
            SurfaceScope::Administration,
            CertificateIdentity::new(
                "fingerprint-for-trace-query",
                "CN=trace-query-admin",
                SurfaceScope::Administration,
            )
            .unwrap(),
            UserPrincipal::new(principal, UserPrincipalKind::Human).unwrap(),
            Permission::InspectPlans,
            SecurityAuditOutcome::Allowed,
            "trace query access authorized",
        )
        .unwrap(),
    )
}

#[test]
fn trace_query_spec_rejects_unbounded_limits_and_zero_filters() {
    let mut spec = TraceQuerySpec::new(TraceQueryFilter::default());
    spec.limit = 0;
    assert!(spec.validate().is_err());

    spec.limit = TRACE_QUERY_MAX_LIMIT + 1;
    assert!(spec.validate().is_err());

    spec.limit = TRACE_QUERY_DEFAULT_LIMIT;
    spec.filter.trace_id = Some(TraceId::new(0));
    assert!(spec.validate().is_err());

    spec.filter.trace_id = None;
    spec.filter.lsn_range = Some(TraceQueryLsnRange::new(20, 10));
    assert!(spec.validate().is_err());
}

#[test]
fn trace_query_filters_by_family_lsn_catalog_procedure_and_principal() {
    let mut emitter = EventEmitter::new(InMemoryEventSink::new());

    emitter
        .emit(EventCorrelation::empty(), protocol_event(101))
        .unwrap();

    let mut wal_corr = EventCorrelation::empty();
    wal_corr.catalog_version = Some(CatalogVersion::new(7));
    wal_corr.catalog_object_id = Some(CatalogObjectId::new(42));
    emitter.emit(wal_corr, wal_event(102, 500)).unwrap();

    let mut audit_corr = EventCorrelation::empty();
    audit_corr.request_id = Some(RequestId::new(1));
    audit_corr.session_id = Some(SessionId::new(2));
    emitter
        .emit(audit_corr, security_event(103, "principal:operator"))
        .unwrap();

    let sink = emitter.sink();

    let mut spec = TraceQuerySpec::new(TraceQueryFilter {
        family: Some(TraceEventFamily::Wal),
        lsn_range: Some(TraceQueryLsnRange::new(400, 600)),
        catalog_version: Some(CatalogVersion::new(7)),
        procedure_id: Some(ProcedureId::new(42)),
        ..TraceQueryFilter::default()
    });
    spec.include_total_count = true;

    let result = sink.inspect_trace_events(&spec).unwrap();
    assert_eq!(result.metadata.returned_rows, 1);
    assert_eq!(result.metadata.total_matching_rows, Some(1));
    assert_eq!(result.rows[0].family, TraceEventFamily::Wal);

    let principal_spec = TraceQuerySpec::new(TraceQueryFilter {
        principal: Some("principal:operator".to_string()),
        ..TraceQueryFilter::default()
    });
    let result = sink.inspect_trace_events(&principal_spec).unwrap();
    assert_eq!(result.metadata.returned_rows, 1);
    assert_eq!(result.rows[0].family, TraceEventFamily::SecurityAudit);
}

#[test]
fn trace_query_limit_offset_and_permission_matrix_are_bounded_and_deterministic() {
    let mut emitter = EventEmitter::new(InMemoryEventSink::new());
    emitter
        .emit(EventCorrelation::empty(), protocol_event(101))
        .unwrap();
    emitter
        .emit(EventCorrelation::empty(), protocol_event(102))
        .unwrap();
    emitter
        .emit(EventCorrelation::empty(), protocol_event(103))
        .unwrap();

    let mut spec = TraceQuerySpec::new(TraceQueryFilter {
        family: Some(TraceEventFamily::Protocol),
        ..TraceQueryFilter::default()
    });
    spec.limit = 1;
    spec.offset = 1;
    spec.include_total_count = true;

    let result = emitter.sink().inspect_trace_events(&spec).unwrap();
    assert_eq!(result.metadata.returned_rows, 1);
    assert_eq!(result.metadata.total_matching_rows, Some(3));
    assert!(result.metadata.truncated);
    assert!(result.metadata.ordered_by_event_id_ascending);
    assert_eq!(result.rows[0].envelope.event_id.get(), 2);

    let matrix = result.metadata.permission_matrix;
    assert!(matrix.audit_required);
    assert!(matrix.permits(SurfaceScope::Administration, Permission::InspectPlans));
    assert!(matrix.permits(SurfaceScope::Administration, Permission::ManageSecurity));
    assert!(!matrix.permits(SurfaceScope::Application, Permission::InspectPlans));
    assert!(!matrix.permits(SurfaceScope::MonitoringAgent, Permission::InspectPlans));
}

#[test]
fn trace_event_family_mapping_covers_boundary_variants() {
    let trace_id = TraceId::new(9001);
    let cert = CertificateIdentity::new(
        "fingerprint-for-family-mapping",
        "CN=trace-query-admin",
        SurfaceScope::Administration,
    )
    .unwrap();
    let principal = UserPrincipal::new("principal:family", UserPrincipalKind::Human).unwrap();

    let execution_transition = TraceEvent::ExecutionTransition(ExecutionTransitionTrace {
        trace_id,
        invocation_id: InvocationId::new(1),
        request_id: Some(RequestId::new(2)),
        session_id: Some(SessionId::new(3)),
        transaction_id: None,
        completion_code: None,
        prev_phase: None,
        next_phase: None,
        durable_lsn: None,
        reason_code: TransitionReasonCode::NORMAL_PROGRESS,
        reason: "execution progressed".to_string(),
    });
    assert_eq!(
        TraceEventFamily::of(&execution_transition),
        TraceEventFamily::ProcedureInvocation
    );

    assert_eq!(
        TraceEventFamily::of(&TraceEvent::CorruptionBoundary(CorruptionBoundaryTrace {
            trace_id,
            boundary_lsn: 42,
            reason: "detected corruption fence".to_string(),
        })),
        TraceEventFamily::Wal
    );
    assert_eq!(
        TraceEventFamily::of(&TraceEvent::RecoveryStartup(RecoveryTrace {
            trace_id,
            last_durable_lsn: 101,
            corruption_boundary_lsn: Some(100),
        })),
        TraceEventFamily::Recovery
    );
    assert_eq!(
        TraceEventFamily::of(&TraceEvent::CatalogMutation(CatalogMutationTrace {
            trace_id,
            catalog_version: CatalogVersion::new(7),
            object_id: Some(CatalogObjectId::new(77)),
            action: "create proc".to_string(),
        })),
        TraceEventFamily::ManifestCatalog
    );
    assert_eq!(
        TraceEventFamily::of(&TraceEvent::AuthorizationDenied(AuthorizationDeniedTrace {
            trace_id,
            denied_permission: "inspect_plans".to_string(),
            reason: "permission denied".to_string(),
        })),
        TraceEventFamily::SecurityAudit
    );
    assert_eq!(
        TraceEventFamily::of(&TraceEvent::AdminOperation(
            AdminOperationTrace::new(
                trace_id,
                SurfaceScope::Administration,
                cert,
                principal,
                AdminOperation::InspectPlans,
                Permission::InspectPlans,
                true,
                "authorized admin action",
            )
            .unwrap()
        )),
        TraceEventFamily::AdminAudit
    );
    assert_eq!(
        TraceEventFamily::of(&TraceEvent::Audit(AuditTrace {
            trace_id,
            actor: "principal:family".to_string(),
            object: "trace.query".to_string(),
            action: "inspect".to_string(),
        })),
        TraceEventFamily::AdminAudit
    );
    assert_eq!(
        TraceEventFamily::of(&TraceEvent::Resource(crate::ResourceTrace {
            trace_id,
            memory_bytes: 1,
            temp_bytes: 0,
        })),
        TraceEventFamily::Resource
    );
    assert_eq!(
        TraceEventFamily::of(&TraceEvent::IoBudgetDecision(
            IoBudgetDecisionTrace::from_budget_request(
                trace_id,
                PipelineClass::WalAppend,
                IoPipelineStage::Hot,
                ResourceBudget::new(1024, 1024, 4),
                100,
                100,
                1,
                "io budget accepted",
            )
            .unwrap()
        )),
        TraceEventFamily::Io
    );
    assert_eq!(
        TraceEventFamily::of(&TraceEvent::GpuPolicyDecision(
            crate::GpuPolicyDecisionTrace::from_policy(
                trace_id,
                PipelineClass::BatchAnalytics,
                GpuExecutionPolicy::BatchAnalyticsOnly,
                true,
                "gpu policy accepted",
            )
            .unwrap()
        )),
        TraceEventFamily::Gpu
    );
    assert_eq!(
        TraceEventFamily::of(&TraceEvent::Mvcc(MvccTrace {
            trace_id,
            snapshot_ts: 1,
            visible: true,
        })),
        TraceEventFamily::Transaction
    );
    assert_eq!(
        TraceEventFamily::of(&TraceEvent::Decision(DecisionTrace {
            trace_id,
            decision: CriticalDecisionKind::PlanSelection,
            reason: "selected deterministic plan".to_string(),
        })),
        TraceEventFamily::Decision
    );
}
