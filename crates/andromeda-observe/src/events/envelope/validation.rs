use andromeda_core::AndromedaResult;

use crate::events::{EventEnvelope, TraceEvent, observe_error};

use super::{correlation, secret_safety};

pub(super) fn validate(envelope: &EventEnvelope) -> AndromedaResult<()> {
    validate_identity(envelope)?;
    validate_transition_payloads(envelope)?;

    match &envelope.event {
        TraceEvent::Decision(trace) if !trace.has_explanation() => Err(observe_error(
            "critical decision traces require a non-empty reason",
        )),
        TraceEvent::Wal(trace) if !trace.proves_durable_boundary() => Err(observe_error(
            "legacy WAL traces require non-zero durable LSN evidence",
        )),
        TraceEvent::WalEvent(trace) if !trace.has_lsn_evidence() => Err(observe_error(
            "WAL append/flush traces require explicit LSN evidence",
        )),
        TraceEvent::CommitVisible(trace) if !trace.proves_wal_before_visible_commit() => Err(
            observe_error("commit-visible traces require non-zero durable commit LSN evidence"),
        ),
        TraceEvent::RollbackDurable(trace) if !trace.proves_durable_rollback() => Err(
            observe_error("rollback-durable traces require non-zero rollback LSN evidence"),
        ),
        TraceEvent::RecoveryStartup(trace) if !trace.proves_recovery_boundary() => Err(
            observe_error("recovery startup traces require non-zero durable LSN evidence"),
        ),
        TraceEvent::Manifest(trace) if !trace.has_reason() => Err(observe_error(
            "manifest validation/switch traces require a non-empty reason",
        )),
        TraceEvent::Manifest(trace) if !trace.has_wal_anchor_evidence() => Err(observe_error(
            "manifest validation/switch traces require manifest epoch and WAL anchor evidence",
        )),
        TraceEvent::CatalogMutation(trace) if !trace.has_action() => Err(observe_error(
            "catalog mutation traces require a non-empty action",
        )),
        TraceEvent::FrameRejection(trace) if !trace.has_reason() => Err(observe_error(
            "frame rejection traces require a non-empty reason",
        )),
        TraceEvent::FrameRejection(trace) if !trace.has_frame_evidence() => Err(observe_error(
            "frame rejection traces require stream_id and frame_type evidence",
        )),
        TraceEvent::StreamRoleRejection(trace) if !trace.has_reason() => Err(observe_error(
            "stream role rejection traces require a non-empty reason",
        )),
        TraceEvent::StreamRoleRejection(trace) if !trace.has_role_evidence() => Err(observe_error(
            "stream role rejection traces require stream_id, observed_role, and expected_role evidence",
        )),
        TraceEvent::Backpressure(trace) if !trace.has_reason() => Err(observe_error(
            "backpressure traces require a non-empty reason",
        )),
        TraceEvent::Backpressure(trace) if !trace.has_pressure_evidence() => Err(observe_error(
            "backpressure traces require retry or queue pressure evidence",
        )),
        TraceEvent::CompletionEmitted(trace) if !trace.has_reason() => Err(observe_error(
            "completion emitted traces require a non-empty reason",
        )),
        TraceEvent::CompletionEmitted(trace) if !trace.has_completion_evidence() => Err(
            observe_error("completion emitted traces require completion code evidence"),
        ),
        TraceEvent::CompletionEmitted(trace) if !trace.proves_committed_completion() => Err(
            observe_error("committed completion traces require non-zero durable LSN evidence"),
        ),
        TraceEvent::ContractRejected(trace) if !trace.has_reason() => Err(observe_error(
            "contract rejected traces require a non-empty reason",
        )),
        TraceEvent::ContractRejected(trace) if !trace.has_contract_evidence() => {
            Err(observe_error(
                "contract rejected traces require contract kind and rejection code evidence",
            ))
        }
        TraceEvent::AuthorizationDenied(trace) if !trace.has_reason() => Err(observe_error(
            "authorization denial traces require a non-empty reason",
        )),
        TraceEvent::AuthorizationDenied(trace) if !trace.has_permission_evidence() => Err(
            observe_error("authorization denial traces require denied permission evidence"),
        ),
        TraceEvent::SecurityAudit(trace) if !trace.has_supported_schema_version() => Err(
            observe_error("security audit traces require the V0 event schema version"),
        ),
        TraceEvent::SecurityAudit(trace) if !trace.has_identity_evidence() => Err(observe_error(
            "security audit traces require certificate and principal identity evidence",
        )),
        TraceEvent::SecurityAudit(trace) if !trace.surface_matches_certificate() => Err(
            observe_error("security audit trace surface must match certificate surface scope"),
        ),
        TraceEvent::SecurityAudit(trace) if !trace.surface_permits_permission() => Err(
            observe_error("security audit traces require surface scope matching permission family"),
        ),
        TraceEvent::SecurityAudit(trace) if !trace.has_policy_version_evidence() => Err(
            observe_error("security audit traces require security policy version evidence"),
        ),
        TraceEvent::SecurityAudit(trace) if !trace.has_reason() => Err(observe_error(
            "security audit traces require a non-empty reason",
        )),
        TraceEvent::AdminOperation(trace) if !trace.has_supported_schema_version() => Err(
            observe_error("admin operation traces require the V0 event schema version"),
        ),
        TraceEvent::AdminOperation(trace) if !trace.surface_permits_operation() => {
            Err(observe_error("surface cannot carry admin operation traces"))
        }
        TraceEvent::AdminOperation(trace) if !trace.has_identity_evidence() => Err(observe_error(
            "admin operation traces require certificate and principal identity evidence",
        )),
        TraceEvent::AdminOperation(trace) if !trace.surface_matches_certificate() => Err(
            observe_error("admin operation trace surface must match certificate surface scope"),
        ),
        TraceEvent::AdminOperation(trace) if !trace.permission_matches_operation() => {
            Err(observe_error(
                "admin operation traces require permission evidence matching the operation",
            ))
        }
        TraceEvent::AdminOperation(trace) if !trace.has_reason() => Err(observe_error(
            "admin operation traces require a non-empty reason",
        )),
        TraceEvent::UnsupportedVersion(trace) if !trace.has_reason() => Err(observe_error(
            "unsupported version traces require a non-empty reason",
        )),
        TraceEvent::UnsupportedVersion(trace) if !trace.has_version_evidence() => {
            Err(observe_error(
                "unsupported version traces require offered, minimum, and maximum version evidence",
            ))
        }
        TraceEvent::SchemaLayoutDecision(trace) if !trace.has_reason() => Err(observe_error(
            "schema/layout decision traces require a non-empty reason",
        )),
        TraceEvent::SchemaLayoutDecision(trace) if !trace.has_schema_layout_evidence() => {
            Err(observe_error(
                "schema/layout decision traces require schema and layout numeric evidence",
            ))
        }
        TraceEvent::CorruptionBoundary(trace) if !trace.proves_boundary() => Err(observe_error(
            "corruption boundary traces require boundary LSN and reason",
        )),
        TraceEvent::Audit(trace) if !trace.is_complete() => Err(observe_error(
            "audit traces require actor, object, and action evidence",
        )),
        TraceEvent::IoPlacementDecision(trace) if !trace.has_reason() => Err(observe_error(
            "IO placement decision traces require a non-empty reason",
        )),
        TraceEvent::PlacementAudit(trace) if !trace.has_reason() => Err(observe_error(
            "placement audit events require a non-empty reason",
        )),
        TraceEvent::PlacementAudit(trace) if !trace.has_transition_evidence() => {
            Err(observe_error(
                "placement audit events require transition-specific segment/extent evidence",
            ))
        }
        TraceEvent::PlacementAudit(trace) if !trace.outcome_matches_transition() => Err(
            observe_error("placement audit event acceptance must match transition semantics"),
        ),
        TraceEvent::IoBudgetDecision(trace) if !trace.has_reason() => Err(observe_error(
            "IO budget decision traces require a non-empty reason",
        )),
        TraceEvent::IoBudgetDecision(trace) if !trace.has_budget_evidence() => Err(observe_error(
            "IO budget decision traces require explicit budget limit evidence",
        )),
        TraceEvent::IoBudgetDecision(trace) if !trace.outcome_matches_budget() => {
            Err(observe_error(
                "IO budget decision outcome must match requested usage and budget limits",
            ))
        }
        TraceEvent::GpuPolicyDecision(trace) if !trace.has_reason() => Err(observe_error(
            "GPU policy decision traces require a non-empty reason",
        )),
        TraceEvent::GpuPolicyDecision(trace) if !trace.outcome_matches_policy() => {
            Err(observe_error(
                "GPU policy decision outcome must match availability, policy, and pipeline",
            ))
        }
        _ => validate_common_guards(envelope),
    }
}

fn validate_identity(envelope: &EventEnvelope) -> AndromedaResult<()> {
    if envelope.event_id.is_zero() {
        return Err(observe_error("observability event_id must be non-zero"));
    }

    if envelope.trace_id.is_zero() {
        return Err(observe_error("observability trace_id must be non-zero"));
    }

    if envelope.trace_id != envelope.event.trace_id() {
        return Err(observe_error(
            "observability envelope trace_id must match payload trace_id",
        ));
    }

    Ok(())
}

fn validate_transition_payloads(envelope: &EventEnvelope) -> AndromedaResult<()> {
    match &envelope.event {
        TraceEvent::TransactionTransition(trace) => trace.validate(),
        TraceEvent::ExecutionTransition(trace) => trace.validate(),
        _ => Ok(()),
    }
}

fn validate_common_guards(envelope: &EventEnvelope) -> AndromedaResult<()> {
    secret_safety::validate(envelope)?;
    correlation::validate(envelope)
}
