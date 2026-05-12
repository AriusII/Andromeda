use std::time::SystemTime;

use andromeda_audit::{
    EventSchemaVersion, HadrAuditEvent, HadrAuditTrace, SecurityPolicyVersionEvidence, TraceId,
};

fn policy_v1() -> SecurityPolicyVersionEvidence {
    SecurityPolicyVersionEvidence::new(
        1,
        "sha256:2222222222222222222222222222222222222222222222222222222222222222",
    )
    .expect("policy digest must be canonical")
}

fn valid_cluster_event() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: true,
        policy_version: Some(policy_v1()),
        principal_id: Some("operator:cluster".to_string()),
        epoch: 11,
        node_id: 3,
        quorum_summary: "quorum_met".to_string(),
        fencing_summary: "fencing_not_required".to_string(),
        last_valid_lsn: 9_000,
        decision: "allow".to_string(),
        result: "success".to_string(),
        rejection_code: None,
        error_code: None,
        reason: "cluster event accepted".to_string(),
        decision_trace_id: Some(TraceId::new(222)),
        recovery_report_id: Some(7),
    }
}

fn cluster_event_missing_policy_and_principal() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: true,
        policy_version: None,
        principal_id: None,
        epoch: 11,
        node_id: 3,
        quorum_summary: "quorum_met".to_string(),
        fencing_summary: "fencing_not_required".to_string(),
        last_valid_lsn: 9_000,
        decision: "allow".to_string(),
        result: "success".to_string(),
        rejection_code: None,
        error_code: None,
        reason: "cluster event accepted".to_string(),
        decision_trace_id: Some(TraceId::new(222)),
        recovery_report_id: Some(7),
    }
}

fn cluster_event_with_mismatched_principal() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: true,
        policy_version: Some(policy_v1()),
        principal_id: Some("operator:other".to_string()),
        epoch: 11,
        node_id: 3,
        quorum_summary: "quorum_met".to_string(),
        fencing_summary: "fencing_not_required".to_string(),
        last_valid_lsn: 9_000,
        decision: "allow".to_string(),
        result: "success".to_string(),
        rejection_code: None,
        error_code: None,
        reason: "cluster event accepted".to_string(),
        decision_trace_id: Some(TraceId::new(222)),
        recovery_report_id: Some(7),
    }
}

fn cluster_event_downgraded_without_principal_binding() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: false,
        policy_version: None,
        principal_id: None,
        epoch: 11,
        node_id: 3,
        quorum_summary: "quorum_met".to_string(),
        fencing_summary: "fencing_not_required".to_string(),
        last_valid_lsn: 9_000,
        decision: "allow".to_string(),
        result: "success".to_string(),
        rejection_code: None,
        error_code: None,
        reason: "cluster event accepted".to_string(),
        decision_trace_id: Some(TraceId::new(222)),
        recovery_report_id: Some(7),
    }
}

fn cluster_event_non_security_sensitive_with_principal_binding() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: false,
        policy_version: None,
        principal_id: Some("operator:cluster".to_string()),
        epoch: 11,
        node_id: 3,
        quorum_summary: "quorum_met".to_string(),
        fencing_summary: "fencing_not_required".to_string(),
        last_valid_lsn: 9_000,
        decision: "allow".to_string(),
        result: "success".to_string(),
        rejection_code: None,
        error_code: None,
        reason: "non secret HADR trace payload accepted".to_string(),
        decision_trace_id: Some(TraceId::new(222)),
        recovery_report_id: Some(7),
    }
}

fn cluster_event_with_secret_principal() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: true,
        policy_version: Some(policy_v1()),
        principal_id: Some("token=cluster-secret".to_string()),
        epoch: 11,
        node_id: 3,
        quorum_summary: "quorum_met".to_string(),
        fencing_summary: "fencing_not_required".to_string(),
        last_valid_lsn: 9_000,
        decision: "allow".to_string(),
        result: "success".to_string(),
        rejection_code: None,
        error_code: None,
        reason: "cluster event accepted".to_string(),
        decision_trace_id: Some(TraceId::new(222)),
        recovery_report_id: Some(7),
    }
}

fn cluster_event_with_secret_reason() -> HadrAuditEvent {
    HadrAuditEvent::ClusterEventTraceV0 {
        schema_version: EventSchemaVersion::V0,
        security_sensitive: true,
        policy_version: Some(policy_v1()),
        principal_id: Some("operator:cluster".to_string()),
        epoch: 11,
        node_id: 3,
        quorum_summary: "quorum_met".to_string(),
        fencing_summary: "fencing_not_required".to_string(),
        last_valid_lsn: 9_000,
        decision: "allow".to_string(),
        result: "success".to_string(),
        rejection_code: None,
        error_code: None,
        reason: "password=forbidden".to_string(),
        decision_trace_id: Some(TraceId::new(222)),
        recovery_report_id: Some(7),
    }
}

#[test]
fn cluster_event_trace_v0_validates_required_fields() {
    let trace = HadrAuditTrace::new(
        TraceId::new(1_000),
        "operator:cluster",
        valid_cluster_event(),
        SystemTime::now(),
        1,
    );
    assert!(trace.validate());
}

#[test]
fn cluster_event_trace_v0_rejects_missing_policy_and_principal_for_security_sensitive_events() {
    let trace = HadrAuditTrace::new(
        TraceId::new(1_001),
        "operator:cluster",
        cluster_event_missing_policy_and_principal(),
        SystemTime::now(),
        1,
    );

    assert!(!trace.validate());
}

#[test]
fn cluster_event_trace_v0_rejects_principal_identity_mismatch() {
    let trace = HadrAuditTrace::new(
        TraceId::new(1_003),
        "operator:cluster",
        cluster_event_with_mismatched_principal(),
        SystemTime::now(),
        1,
    );

    assert!(!trace.validate());
}

#[test]
fn cluster_event_trace_v0_rejects_security_flag_downgrade_without_principal_binding() {
    let trace = HadrAuditTrace::new(
        TraceId::new(1_004),
        "operator:cluster",
        cluster_event_downgraded_without_principal_binding(),
        SystemTime::now(),
        1,
    );

    assert!(!trace.validate());
}

#[test]
fn cluster_event_trace_v0_accepts_non_security_sensitive_non_secret_payload() {
    let trace = HadrAuditTrace::new(
        TraceId::new(1_005),
        "operator:cluster",
        cluster_event_non_security_sensitive_with_principal_binding(),
        SystemTime::now(),
        1,
    );

    assert!(trace.validate());
}

#[test]
fn cluster_event_trace_v0_rejects_secret_reason_markers() {
    let trace = HadrAuditTrace::new(
        TraceId::new(1_002),
        "operator:cluster",
        cluster_event_with_secret_reason(),
        SystemTime::now(),
        1,
    );

    assert!(!trace.validate());
}

#[test]
fn cluster_event_trace_v0_rejects_secret_principal_markers() {
    let trace = HadrAuditTrace::new(
        TraceId::new(1_006),
        "operator:cluster",
        cluster_event_with_secret_principal(),
        SystemTime::now(),
        1,
    );

    assert!(!trace.validate());
}
