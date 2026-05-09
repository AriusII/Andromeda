use andromeda_audit::{
    CertificateIdentity, Permission, SurfaceScope, UserPrincipal, UserPrincipalKind,
};
use andromeda_exec::LocalVerticalRuntime;
use andromeda_observability::TraceId;
use andromeda_observe::{
    EventCorrelation, EventEmitter, EventEnvelope, EventId, EventSink, TraceEvent,
};
use andromeda_security::{PrincipalBinding, PrincipalRegistry};
use andromeda_types::TransactionId;
use andromeda_wal::InMemoryWal;

pub const APP_FINGERPRINT: &str = "sha256-fingerprint-abc123";
pub const ADMIN_FINGERPRINT: &str = "sha256-fingerprint-admin";
pub const UNKNOWN_FINGERPRINT: &str = "unknown-certificate-fp";

pub fn empty_runtime() -> LocalVerticalRuntime<InMemoryWal> {
    LocalVerticalRuntime::new(InMemoryWal::new())
}

pub fn application_registry() -> PrincipalRegistry {
    registry(vec![binding(
        APP_FINGERPRINT,
        "CN=svc-inventory",
        SurfaceScope::Application,
        "svc-inventory-prod",
        UserPrincipalKind::Service,
        vec![Permission::ExecuteProcedure],
    )])
}

pub fn administration_registry() -> PrincipalRegistry {
    registry(vec![binding(
        ADMIN_FINGERPRINT,
        "CN=ops-admin",
        SurfaceScope::Administration,
        "ops-admin",
        UserPrincipalKind::Human,
        vec![Permission::ExecuteProcedure],
    )])
}

pub fn recovery_correlation(durable_lsn: u64) -> EventCorrelation {
    correlation(None, Some(durable_lsn))
}

pub fn transaction_correlation(
    transaction_id: TransactionId,
    durable_lsn: u64,
) -> EventCorrelation {
    correlation(Some(transaction_id), Some(durable_lsn))
}

pub fn recovery_startup(trace_id: TraceId, last_durable_lsn: u64) -> TraceEvent {
    TraceEvent::RecoveryStartup(andromeda_observe::RecoveryTrace {
        trace_id,
        last_durable_lsn,
        corruption_boundary_lsn: None,
    })
}

pub fn wal_replay(trace_id: TraceId, durable_lsn: u64) -> TraceEvent {
    TraceEvent::Wal(andromeda_observe::WalTrace {
        trace_id,
        transaction_id: None,
        durable_lsn,
    })
}

pub fn incomplete_transaction_wal_event(
    trace_id: TraceId,
    transaction_id: TransactionId,
    appended_lsn: u64,
) -> TraceEvent {
    TraceEvent::WalEvent(andromeda_observe::WalEventTrace {
        trace_id,
        transaction_id: Some(transaction_id),
        operation: andromeda_observe::WalOperation::Append,
        appended_lsn,
        durable_lsn: Some(appended_lsn),
    })
}

pub fn emit_trace<S: EventSink>(
    emitter: &mut EventEmitter<S>,
    event_id: u128,
    correlation: EventCorrelation,
    event: TraceEvent,
    expectation: &str,
) -> EventId {
    let envelope = EventEnvelope::new(EventId::new(event_id), correlation, event)
        .expect("test trace envelope should be valid");

    emitter.emit_envelope(envelope).expect(expectation)
}

fn registry(bindings: Vec<PrincipalBinding>) -> PrincipalRegistry {
    let mut registry = PrincipalRegistry::new();
    for binding in bindings {
        registry
            .register(binding)
            .expect("principal binding should be registered");
    }
    registry
}

fn binding(
    fingerprint: &str,
    subject: &str,
    surface: SurfaceScope,
    principal_id: &str,
    principal_kind: UserPrincipalKind,
    permissions: Vec<Permission>,
) -> PrincipalBinding {
    let certificate = CertificateIdentity::new(fingerprint, subject, surface)
        .expect("certificate identity should be valid");
    let principal = UserPrincipal::new(principal_id, principal_kind)
        .expect("principal identity should be valid");

    PrincipalBinding::new(certificate, principal, permissions)
        .expect("principal binding should be valid")
}

fn correlation(
    transaction_id: Option<TransactionId>,
    durable_lsn: Option<u64>,
) -> EventCorrelation {
    EventCorrelation {
        request_id: None,
        session_id: None,
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        transaction_id,
        durable_lsn,
        protocol: None,
    }
}
