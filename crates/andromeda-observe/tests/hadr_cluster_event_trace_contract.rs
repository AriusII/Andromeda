use std::time::SystemTime;

use andromeda_observability::{
    EventCorrelation, EventId, EventSchemaVersion, TraceEventFamily, TraceId,
};
use andromeda_observe::{
    DurableAuditEventFamily, DurableAuditPrincipalBinding, DurableAuditReplayBehavior,
    DurableAuditRetentionBoundary, EventEnvelope, HadrAuditEvent, HadrAuditTrace,
    PendingDurableAuditRecord, Permission, SecurityPolicyVersionEvidence, SurfaceScope, TraceEvent,
};
use andromeda_types::{RequestId, SessionId};

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

fn cluster_event_non_security_sensitive() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: false,
        policy_version: None,
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
        reason: "cluster observation accepted without secret payload material".to_string(),
        decision_trace_id: Some(TraceId::new(88)),
        recovery_report_id: Some(9),
    }
}

fn request_correlation(event_id: u128) -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        transaction_id: None,
        durable_lsn: None,
        protocol: None,
    }
}

fn cluster_envelope(event_id: u128, trace_id: u128, event: HadrAuditEvent) -> EventEnvelope {
    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(event_id),
        TraceEvent::HadrCluster(HadrAuditTrace::new(
            TraceId::new(trace_id),
            "operator:cluster",
            event,
            SystemTime::now(),
            1,
        )),
    )
    .expect("HADR cluster event envelope is valid")
}

fn cluster_principal_binding(event_id: u128) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "operator:cluster".to_string(),
        certificate_fingerprint: Some(format!("sha256:hadr-cluster-{event_id}")),
        surface: Some(SurfaceScope::Cluster),
        permission: Some(Permission::ClusterPromote),
        policy_version: Some(policy_v1()),
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
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

#[test]
fn hadr_cluster_event_trace_v0_security_false_still_requires_matching_principal() {
    let envelope = EventEnvelope::new(
        EventId::new(4),
        EventCorrelation::empty(),
        TraceEvent::HadrCluster(HadrAuditTrace::new(
            TraceId::new(80),
            "operator:cluster",
            cluster_event_non_security_sensitive(),
            SystemTime::now(),
            1,
        )),
    )
    .expect("non-security-sensitive HADR cluster trace remains principal-bound");

    assert!(matches!(envelope.event, TraceEvent::HadrCluster(_)));
}

#[test]
fn hadr_cluster_event_trace_v0_maps_to_durable_hadr_decision() {
    let record = PendingDurableAuditRecord::new(
        1,
        cluster_principal_binding(5),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        cluster_envelope(5, 81, valid_cluster_event()),
    )
    .expect("HADR cluster event trace is durable-audit eligible");

    assert_eq!(
        record.identity.family,
        DurableAuditEventFamily::HadrDecision
    );
}

#[test]
fn durable_hadr_cluster_event_rejects_binding_principal_mismatch() {
    let mut binding = cluster_principal_binding(6);
    binding.principal_id = "operator:other".to_string();

    let err = PendingDurableAuditRecord::new(
        2,
        binding,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        cluster_envelope(6, 82, valid_cluster_event()),
    )
    .expect_err("durable HADR mapping must bind to the enclosing trace principal");

    assert!(
        err.message()
            .contains("principal_id must match HADR cluster trace")
    );
}

#[test]
fn durable_hadr_cluster_event_accepts_non_secret_payload_as_hadr_decision() {
    let record = PendingDurableAuditRecord::new(
        3,
        cluster_principal_binding(7),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        cluster_envelope(7, 83, cluster_event_non_security_sensitive()),
    )
    .expect("non-secret HADR cluster evidence remains durable HADR decision evidence");

    assert_eq!(
        record.identity.family,
        DurableAuditEventFamily::HadrDecision
    );
}
