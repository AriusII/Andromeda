use crate::support::*;

#[test]
fn permission_check_failed_event_records_missing_permission() {
    let event = permission_check_failed_event("henry", "execute_procedure", "");

    assert_eq!(event.type_label(), "permission_check_failed");
    assert_eq!(event.affected_principal_id(), "CN=test-principal-henry");
    assert!(matches!(
        &event,
        AdmissionAuditEvent::PermissionCheckFailed {
            required_permission,
            actual_permission_set,
            ..
        } if required_permission == "execute_procedure" && actual_permission_set.is_empty()
    ));
    assert!(event.is_valid());
}

#[test]
fn permission_check_failed_event_partial_permission_set_shows_gap() {
    let event =
        permission_check_failed_event("iris", "execute_procedure,debug_procedure", "read_contract");

    assert_eq!(event.type_label(), "permission_check_failed");
    assert!(matches!(
        &event,
        AdmissionAuditEvent::PermissionCheckFailed {
            required_permission,
            actual_permission_set,
            ..
        } if required_permission.contains("execute_procedure") && actual_permission_set == "read_contract"
    ));
    assert!(event.is_valid());
}

#[test]
fn permission_check_failed_event_immutably_binds_principal() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("jack");
    let principal_id = principal.principal_id.clone();

    let event = AdmissionAuditEvent::PermissionCheckFailed {
        trace_id,
        procedure_id: proc_id,
        affected_principal: principal,
        required_permission: "backup".to_string(),
        actual_permission_set: "read_contract,read_procedure_store".to_string(),
        event_timestamp: now(),
    };

    // Principal is immutably bound; cannot be changed after event creation
    assert_eq!(event.affected_principal_id(), principal_id);
    assert!(event.is_valid());
}

#[test]
fn request_throttled_event_buffer_pool_full_provides_retry_delay() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();

    let event = request_throttled_event(BackpressureReason::BufferPoolFull, 250);

    assert_eq!(event.type_label(), "request_throttled");
    assert_eq!(event.trace_id(), trace_id);
    assert_eq!(event.procedure_id(), proc_id);
    assert!(matches!(
        event,
        AdmissionAuditEvent::RequestThrottled {
            backpressure_reason: BackpressureReason::BufferPoolFull,
            retry_after_ms: 250,
            ..
        }
    ));
    assert!(event.is_valid());
}

#[test]
fn request_throttled_event_network_congestion_signals_temporary_condition() {
    let event = request_throttled_event(BackpressureReason::NetworkCongestion, 500);

    assert_eq!(event.type_label(), "request_throttled");
    assert!(matches!(
        event,
        AdmissionAuditEvent::RequestThrottled {
            backpressure_reason: BackpressureReason::NetworkCongestion,
            ..
        }
    ));
    assert!(event.is_valid());
}

#[test]
fn request_throttled_event_queue_overload_variant_distinct_from_others() {
    let event = request_throttled_event(BackpressureReason::QueueOverload, 1000);

    assert_eq!(event.type_label(), "request_throttled");
    assert!(matches!(
        event,
        AdmissionAuditEvent::RequestThrottled {
            backpressure_reason: BackpressureReason::QueueOverload,
            retry_after_ms: 1000,
            ..
        }
    ));
    assert!(event.is_valid());
}

#[test]
fn procedure_dispatch_authorized_event_binds_mtls_certificate_identity() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();

    let event = procedure_dispatch_authorized_event(
        "kelly",
        "Application",
        "sha256:abc123def456 CN=andromeda-client",
    );

    assert_eq!(event.type_label(), "procedure_dispatch_authorized");
    assert_eq!(event.trace_id(), trace_id);
    assert_eq!(event.procedure_id(), proc_id);
    assert_eq!(event.affected_principal_id(), "CN=test-principal-kelly");
    assert!(matches!(
        &event,
        AdmissionAuditEvent::ProcedureDispatchAuthorized {
            surface_plane,
            certificate_identity,
            ..
        } if surface_plane == "Application" && certificate_identity.contains("sha256:abc123def456")
    ));
    assert!(event.is_valid());
}

#[test]
fn procedure_dispatch_authorized_event_immutably_binds_principal_at_dispatch() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("liam");
    let expected_principal_id = principal.principal_id.clone();

    let event = AdmissionAuditEvent::ProcedureDispatchAuthorized {
        trace_id,
        procedure_id: proc_id,
        surface_plane: "Administration".to_string(),
        certificate_identity: "CN=admin-user".to_string(),
        affected_principal: principal,
        dispatch_timestamp: now(),
    };

    // Principal immutably bound at dispatch time; cannot be changed
    assert_eq!(event.affected_principal_id(), expected_principal_id);
    assert_eq!(event.affected_principal_id(), "CN=test-principal-liam");
    assert!(event.is_valid());
}

#[test]
fn procedure_dispatch_authorized_event_records_all_surface_planes() {
    let surfaces = vec![
        "Application",
        "Administration",
        "Cluster",
        "BackupAgent",
        "MonitoringAgent",
    ];

    for surface in surfaces {
        let event = procedure_dispatch_authorized_event("megan", surface, "CN=test-user");

        assert_eq!(event.type_label(), "procedure_dispatch_authorized");
        assert!(matches!(
            &event,
            AdmissionAuditEvent::ProcedureDispatchAuthorized {
                surface_plane,
                ..
            } if surface_plane == surface
        ));
        assert!(event.is_valid());
    }
}
