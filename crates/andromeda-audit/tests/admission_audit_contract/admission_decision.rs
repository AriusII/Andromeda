use crate::support::*;

#[test]
fn admission_decision_event_accepted_decision_admits_procedure() {
    let trace_id = test_trace_id();

    let event = admission_decision_event(
        "diana",
        AdmissionDecisionKind::Accepted,
        "contract valid, principal authorized, quota available",
    );

    assert_eq!(event.type_label(), "admission_decision");
    assert_eq!(event.trace_id(), trace_id);
    assert!(matches!(
        event,
        AdmissionAuditEvent::AdmissionDecision {
            decision: AdmissionDecisionKind::Accepted,
            ..
        }
    ));
    assert!(event.is_valid());
}

#[test]
fn admission_decision_event_rejected_quota_exceeded_carries_limit_info() {
    let event = admission_decision_event(
        "eve",
        AdmissionDecisionKind::RejectedQuotaExceeded,
        "principal quota limit exceeded: 1000 active requests, max=500",
    );

    assert_eq!(event.type_label(), "admission_decision");
    assert!(matches!(
        event,
        AdmissionAuditEvent::AdmissionDecision {
            decision: AdmissionDecisionKind::RejectedQuotaExceeded,
            ..
        }
    ));
    assert!(event.is_valid());
}

#[test]
fn admission_decision_event_rejected_health_state_blocks_execution() {
    let event = admission_decision_event(
        "frank",
        AdmissionDecisionKind::RejectedHealthState,
        "system in maintenance mode; admission suspended",
    );

    assert_eq!(event.type_label(), "admission_decision");
    assert!(matches!(
        event,
        AdmissionAuditEvent::AdmissionDecision {
            decision: AdmissionDecisionKind::RejectedHealthState,
            ..
        }
    ));
    assert!(event.is_valid());
}

#[test]
fn admission_decision_event_rejected_unknown_procedure_terminates_early() {
    let event = admission_decision_event(
        "grace",
        AdmissionDecisionKind::RejectedUnknownProcedure,
        "procedure store lookup failed; procedure not found in catalog v42",
    );

    assert_eq!(event.type_label(), "admission_decision");
    assert!(matches!(
        event,
        AdmissionAuditEvent::AdmissionDecision {
            decision: AdmissionDecisionKind::RejectedUnknownProcedure,
            ..
        }
    ));
    assert!(event.is_valid());
}

#[test]
fn admission_decision_kind_classification_is_consistent() {
    let accepted = AdmissionDecisionKind::Accepted;
    assert!(accepted.is_accepted());
    assert!(!accepted.is_rejected());

    let rejected_quota = AdmissionDecisionKind::RejectedQuotaExceeded;
    assert!(!rejected_quota.is_accepted());
    assert!(rejected_quota.is_rejected());
}
