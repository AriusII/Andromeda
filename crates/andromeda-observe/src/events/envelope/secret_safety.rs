use andromeda_error::AndromedaResult;

use crate::events::{EventEnvelope, TraceEvent, contains_sensitive_marker, observe_error};

pub(super) fn validate(envelope: &EventEnvelope) -> AndromedaResult<()> {
    if event_has_sensitive_marker(&envelope.event) {
        return Err(observe_error(
            "observability text fields must not include secrets, private key material, tokens, passwords, or payload bodies",
        ));
    }

    Ok(())
}

fn event_has_sensitive_marker(event: &TraceEvent) -> bool {
    match event {
        TraceEvent::Decision(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::CatalogMutation(trace) => contains_sensitive_marker(&trace.action),
        TraceEvent::FrameRejection(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::StreamRoleRejection(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::Backpressure(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::CompletionEmitted(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::ContractRejected(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::AuthorizationDenied(trace) => {
            contains_sensitive_marker(&trace.reason)
                || contains_sensitive_marker(&trace.denied_permission)
        },
        TraceEvent::SecurityAudit(trace) => trace.contains_sensitive_evidence(),
        TraceEvent::AdminOperation(trace) => trace.contains_sensitive_evidence(),
        TraceEvent::UnsupportedVersion(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::SchemaLayoutDecision(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::CorruptionBoundary(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::Manifest(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::IoPlacementDecision(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::PlacementAudit(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::IoBudgetDecision(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::GpuPolicyDecision(trace) => contains_sensitive_marker(&trace.reason),
        TraceEvent::GpuExecution(trace) => trace.contains_sensitive_evidence(),
        TraceEvent::HadrCluster(trace) => {
            contains_sensitive_marker(&trace.principal) || trace.event.contains_sensitive_evidence()
        },
        TraceEvent::Audit(trace) => {
            contains_sensitive_marker(&trace.actor)
                || contains_sensitive_marker(&trace.object)
                || contains_sensitive_marker(&trace.action)
        },
        TraceEvent::Invocation(_)
        | TraceEvent::Wal(_)
        | TraceEvent::WalEvent(_)
        | TraceEvent::CommitVisible(_)
        | TraceEvent::RollbackDurable(_)
        | TraceEvent::RecoveryStartup(_)
        | TraceEvent::Mvcc(_)
        | TraceEvent::Resource(_)
        | TraceEvent::TransactionTransition(_)
        | TraceEvent::ExecutionTransition(_) => false,
    }
}
