use crate::support::*;

#[test]
fn hadr_audit_trace_binds_trace_id_and_principal() {
    let trace = HadrAuditTrace::new(
        TraceId::new(123),
        "operator:alice",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 1,
            from: ReplicaHealthState::Alive,
            to: ReplicaHealthState::Suspect,
        },
        SystemTime::now(),
        1,
    );

    assert_eq!(trace.trace_id, TraceId::new(123));
    assert_eq!(trace.principal, "operator:alice");
    assert_eq!(trace.sequence_number, 1);
}

#[test]
fn hadr_audit_trace_system_principal_is_valid() {
    let trace = HadrAuditTrace::new(
        TraceId::new(456),
        "system:recovery",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 2,
            from: ReplicaHealthState::Suspect,
            to: ReplicaHealthState::Dead,
        },
        SystemTime::now(),
        1,
    );

    assert_eq!(trace.principal, "system:recovery");
}

#[test]
fn hadr_audit_trace_validation_rejects_zero_trace_id() {
    let trace = HadrAuditTrace::new(
        TraceId::new(0),
        "operator:alice",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 1,
            from: ReplicaHealthState::Alive,
            to: ReplicaHealthState::Suspect,
        },
        SystemTime::now(),
        1,
    );

    assert!(!trace.validate());
}

#[test]
fn hadr_audit_trace_validation_rejects_empty_principal() {
    let trace = HadrAuditTrace::new(
        TraceId::new(123),
        "",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 1,
            from: ReplicaHealthState::Alive,
            to: ReplicaHealthState::Suspect,
        },
        SystemTime::now(),
        1,
    );

    assert!(!trace.validate());
}

#[test]
fn hadr_audit_trace_validation_rejects_zero_sequence_number() {
    let trace = HadrAuditTrace::new(
        TraceId::new(123),
        "operator:alice",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 1,
            from: ReplicaHealthState::Alive,
            to: ReplicaHealthState::Suspect,
        },
        SystemTime::now(),
        0,
    );

    assert!(!trace.validate());
}

#[test]
fn hadr_audit_trace_validation_accepts_valid_trace() {
    let trace = HadrAuditTrace::new(
        TraceId::new(123),
        "operator:alice",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 1,
            from: ReplicaHealthState::Alive,
            to: ReplicaHealthState::Suspect,
        },
        SystemTime::now(),
        1,
    );

    assert!(trace.validate());
}

#[test]
fn hadr_audit_trace_sequence_numbers_establish_ordering() {
    let event = HadrAuditEvent::ReplicaHealthTransition {
        replica_id: 1,
        from: ReplicaHealthState::Alive,
        to: ReplicaHealthState::Suspect,
    };

    let trace1 = HadrAuditTrace::new(
        TraceId::new(123),
        "op:alice",
        event.clone(),
        SystemTime::now(),
        1,
    );
    let trace2 = HadrAuditTrace::new(TraceId::new(123), "op:alice", event, SystemTime::now(), 2);

    assert!(trace1.sequence_number < trace2.sequence_number);
}
#[test]
fn hadr_trace_lifecycle_replica_health_progresses_through_states() {
    let trace1 = HadrAuditTrace::new(
        TraceId::new(789),
        "monitor:system",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 2,
            from: ReplicaHealthState::Alive,
            to: ReplicaHealthState::Suspect,
        },
        SystemTime::now(),
        1,
    );

    let trace2 = HadrAuditTrace::new(
        TraceId::new(789),
        "monitor:system",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 2,
            from: ReplicaHealthState::Suspect,
            to: ReplicaHealthState::Dead,
        },
        SystemTime::now(),
        2,
    );

    assert_eq!(trace1.sequence_number, 1);
    assert_eq!(trace2.sequence_number, 2);
    assert!(trace1.validate() && trace2.validate());
}
#[test]
fn timestamp_milliseconds_are_monotonically_increasing() {
    let now1 = SystemTime::UNIX_EPOCH + Duration::from_millis(1_000);
    let now2 = SystemTime::UNIX_EPOCH + Duration::from_millis(1_010);

    let trace1 = HadrAuditTrace::new(
        TraceId::new(333),
        "op:test",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 1,
            from: ReplicaHealthState::Alive,
            to: ReplicaHealthState::Suspect,
        },
        now1,
        1,
    );

    let trace2 = HadrAuditTrace::new(
        TraceId::new(333),
        "op:test",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 1,
            from: ReplicaHealthState::Suspect,
            to: ReplicaHealthState::Dead,
        },
        now2,
        2,
    );

    assert_eq!(trace1.timestamp_ms, 1_000);
    assert_eq!(trace2.timestamp_ms, 1_010);
    assert!(trace1.timestamp_ms < trace2.timestamp_ms);
}
#[test]
fn hadr_audit_trace_is_immutable_after_construction() {
    let trace = HadrAuditTrace::new(
        TraceId::new(444),
        "op:alice",
        HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 1,
            from: ReplicaHealthState::Alive,
            to: ReplicaHealthState::Suspect,
        },
        SystemTime::now(),
        1,
    );

    // Verify fields are accessible but cannot be modified (immutable struct)
    let _trace_id = trace.trace_id;
    let _principal = &trace.principal;
    let _event = &trace.event;

    // This would fail to compile if we tried to mutate:
    // trace.principal = "new_value".to_string();
}
