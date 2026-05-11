use std::time::SystemTime;

use andromeda_observability::{
    EventCorrelation, EventId, EventSchemaVersion, TraceEventFamily, TraceId,
};
use andromeda_observe::{
    EventEnvelope, HadrAuditEvent, HadrAuditTrace, SecurityPolicyVersionEvidence, TraceEvent,
};

fn policy_v1() -> SecurityPolicyVersionEvidence {
    SecurityPolicyVersionEvidence::new(
        1,
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    )
    .expect("policy digest must be canonical")
}

fn valid_cluster_event() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: true,
        policy_version: Some(policy_v1()),
        principal_id: Some("operator:cluster".to_string()),
        epoch: 7,
        node_id: 2,
        quorum_summary: "quorum_met".to_string(),
        fencing_summary: "fencing_not_required".to_string(),
        last_valid_lsn: 4_200,
        decision: "allow".to_string(),
        result: "success".to_string(),
        rejection_code: None,
        error_code: None,
        reason: "cluster admission accepted with quorum and healthy replica set".to_string(),
        decision_trace_id: Some(TraceId::new(88)),
        recovery_report_id: Some(9),
    }
}

fn cluster_event_missing_policy_and_principal() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: true,
        policy_version: None,
        principal_id: None,
        epoch: 7,
        node_id: 2,
        quorum_summary: "quorum_met".to_string(),
        fencing_summary: "fencing_not_required".to_string(),
        last_valid_lsn: 4_200,
        decision: "allow".to_string(),
        result: "success".to_string(),
        rejection_code: None,
        error_code: None,
        reason: "cluster admission accepted with quorum and healthy replica set".to_string(),
        decision_trace_id: Some(TraceId::new(88)),
        recovery_report_id: Some(9),
    }
}

fn cluster_event_with_secret_reason() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: true,
        policy_version: Some(policy_v1()),
        principal_id: Some("operator:cluster".to_string()),
        epoch: 7,
        node_id: 2,
        quorum_summary: "quorum_met".to_string(),
        fencing_summary: "fencing_not_required".to_string(),
        last_valid_lsn: 4_200,
        decision: "allow".to_string(),
        result: "success".to_string(),
        rejection_code: None,
        error_code: None,
        reason: "token=cluster-secret".to_string(),
        decision_trace_id: Some(TraceId::new(88)),
        recovery_report_id: Some(9),
    }
}

#[test]
fn hadr_cluster_event_trace_v0_accepts_required_fields_and_maps_to_recovery_family() {
    let envelope = EventEnvelope::new(
        EventId::new(1),
        EventCorrelation::empty(),
        TraceEvent::HadrCluster(HadrAuditTrace::new(
            TraceId::new(77),
            "operator:cluster",
            valid_cluster_event(),
            SystemTime::now(),
            1,
        )),
    )
    .expect("valid HADR cluster trace must pass envelope validation");

    assert_eq!(
        TraceEventFamily::of(&envelope.event),
        TraceEventFamily::Recovery
    );
}

#[test]
fn hadr_cluster_event_trace_v0_rejects_missing_policy_or_principal_for_security_decisions() {
    let err = EventEnvelope::new(
        EventId::new(2),
        EventCorrelation::empty(),
        TraceEvent::HadrCluster(HadrAuditTrace::new(
            TraceId::new(78),
            "operator:cluster",
            cluster_event_missing_policy_and_principal(),
            SystemTime::now(),
            1,
        )),
    )
    .unwrap_err();

    assert!(err.message().contains("security policy/principal evidence"));
}

#[test]
fn hadr_cluster_event_trace_v0_rejects_secret_bearing_reason() {
    let err = EventEnvelope::new(
        EventId::new(3),
        EventCorrelation::empty(),
        TraceEvent::HadrCluster(HadrAuditTrace::new(
            TraceId::new(79),
            "operator:cluster",
            cluster_event_with_secret_reason(),
            SystemTime::now(),
            1,
        )),
    )
    .unwrap_err();

    assert!(err.message().contains("bounded non-secret reason evidence"));
}
