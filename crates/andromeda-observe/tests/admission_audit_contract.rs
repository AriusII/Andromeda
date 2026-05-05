//! Admission audit event contract tests.
//!
//! Comprehensive contract validation for admission control events:
//! - Event immutability and principal binding
//! - All event variants and their properties
//! - Forensic timeline reconstruction
//! - Machine-parseable classification

use std::time::SystemTime;

use andromeda_observe::{
    AdmissionAuditEvent, AdmissionDecisionKind, AffectedPrincipal, BackpressureReason,
    ContractValidationResult, ProcedureId, TraceId,
};

// ============================================================================
// Helper Functions
// ============================================================================

fn test_trace_id() -> TraceId {
    TraceId::new(12345)
}

fn test_procedure_id() -> ProcedureId {
    ProcedureId::new(5001)
}

fn test_principal(name: &str) -> AffectedPrincipal {
    AffectedPrincipal::new(format!("CN=test-principal-{}", name))
}

fn now() -> SystemTime {
    SystemTime::now()
}

// ============================================================================
// ContractValidated Event Tests (3 tests)
// ============================================================================

#[test]
fn contract_validated_event_valid_contract_carries_all_evidence() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("alice");
    let timestamp = now();

    let event = AdmissionAuditEvent::ContractValidated {
        trace_id,
        procedure_id: proc_id,
        input_row_count: 150,
        result: ContractValidationResult::Valid,
        validation_details: "contract hash matches catalog entry".to_string(),
        affected_principal: principal.clone(),
        event_timestamp: timestamp,
    };

    assert_eq!(event.type_label(), "contract_validated");
    assert_eq!(event.trace_id(), trace_id);
    assert_eq!(event.procedure_id(), proc_id);
    assert_eq!(event.affected_principal_id(), "CN=test-principal-alice");
    assert_eq!(event.event_timestamp(), timestamp);
    assert!(event.is_valid());
}

#[test]
fn contract_validated_event_invalid_contract_carries_mismatch_reason() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("bob");

    let event = AdmissionAuditEvent::ContractValidated {
        trace_id,
        procedure_id: proc_id,
        input_row_count: 0,
        result: ContractValidationResult::Invalid,
        validation_details: "contract hash mismatch: expected deadbeef, got cafebabe".to_string(),
        affected_principal: principal,
        event_timestamp: now(),
    };

    assert_eq!(event.type_label(), "contract_validated");
    assert!(matches!(
        event,
        AdmissionAuditEvent::ContractValidated {
            result: ContractValidationResult::Invalid,
            ..
        }
    ));
    assert!(event.is_valid());
}

#[test]
fn contract_validated_event_deprecated_contract_still_valid_for_admission() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("charlie");

    let event = AdmissionAuditEvent::ContractValidated {
        trace_id,
        procedure_id: proc_id,
        input_row_count: 250,
        result: ContractValidationResult::Deprecated,
        validation_details: "contract is deprecated but still usable until EOL 2026-12-31"
            .to_string(),
        affected_principal: principal,
        event_timestamp: now(),
    };

    assert_eq!(event.type_label(), "contract_validated");
    assert!(matches!(
        event,
        AdmissionAuditEvent::ContractValidated {
            result: ContractValidationResult::Deprecated,
            ..
        }
    ));
    // Deprecated is considered valid for admission purposes
    assert!(event.is_valid());
}

// ============================================================================
// AdmissionDecision Event Tests (5 tests)
// ============================================================================

#[test]
fn admission_decision_event_accepted_decision_admits_procedure() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("diana");

    let event = AdmissionAuditEvent::AdmissionDecision {
        trace_id,
        procedure_id: proc_id,
        decision: AdmissionDecisionKind::Accepted,
        reason: "contract valid, principal authorized, quota available".to_string(),
        affected_principal: principal,
        event_timestamp: now(),
    };

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
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("eve");

    let event = AdmissionAuditEvent::AdmissionDecision {
        trace_id,
        procedure_id: proc_id,
        decision: AdmissionDecisionKind::RejectedQuotaExceeded,
        reason: "principal quota limit exceeded: 1000 active requests, max=500".to_string(),
        affected_principal: principal,
        event_timestamp: now(),
    };

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
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("frank");

    let event = AdmissionAuditEvent::AdmissionDecision {
        trace_id,
        procedure_id: proc_id,
        decision: AdmissionDecisionKind::RejectedHealthState,
        reason: "system in maintenance mode; admission suspended".to_string(),
        affected_principal: principal,
        event_timestamp: now(),
    };

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
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("grace");

    let event = AdmissionAuditEvent::AdmissionDecision {
        trace_id,
        procedure_id: proc_id,
        decision: AdmissionDecisionKind::RejectedUnknownProcedure,
        reason: "procedure store lookup failed; procedure not found in catalog v42".to_string(),
        affected_principal: principal,
        event_timestamp: now(),
    };

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

// ============================================================================
// PermissionCheckFailed Event Tests (3 tests)
// ============================================================================

#[test]
fn permission_check_failed_event_records_missing_permission() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("henry");

    let event = AdmissionAuditEvent::PermissionCheckFailed {
        trace_id,
        procedure_id: proc_id,
        affected_principal: principal.clone(),
        required_permission: "execute_procedure".to_string(),
        actual_permission_set: "".to_string(),
        event_timestamp: now(),
    };

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
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("iris");

    let event = AdmissionAuditEvent::PermissionCheckFailed {
        trace_id,
        procedure_id: proc_id,
        affected_principal: principal,
        required_permission: "execute_procedure,debug_procedure".to_string(),
        actual_permission_set: "read_contract".to_string(),
        event_timestamp: now(),
    };

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

// ============================================================================
// RequestThrottled Event Tests (3 tests)
// ============================================================================

#[test]
fn request_throttled_event_buffer_pool_full_provides_retry_delay() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();

    let event = AdmissionAuditEvent::RequestThrottled {
        trace_id,
        procedure_id: proc_id,
        backpressure_reason: BackpressureReason::BufferPoolFull,
        retry_after_ms: 250,
        event_timestamp: now(),
    };

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
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();

    let event = AdmissionAuditEvent::RequestThrottled {
        trace_id,
        procedure_id: proc_id,
        backpressure_reason: BackpressureReason::NetworkCongestion,
        retry_after_ms: 500,
        event_timestamp: now(),
    };

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
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();

    let event = AdmissionAuditEvent::RequestThrottled {
        trace_id,
        procedure_id: proc_id,
        backpressure_reason: BackpressureReason::QueueOverload,
        retry_after_ms: 1000,
        event_timestamp: now(),
    };

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

// ============================================================================
// ProcedureDispatchAuthorized Event Tests (3 tests)
// ============================================================================

#[test]
fn procedure_dispatch_authorized_event_binds_mtls_certificate_identity() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("kelly");

    let event = AdmissionAuditEvent::ProcedureDispatchAuthorized {
        trace_id,
        procedure_id: proc_id,
        surface_plane: "Application".to_string(),
        certificate_identity: "sha256:abc123def456 CN=andromeda-client".to_string(),
        affected_principal: principal.clone(),
        dispatch_timestamp: now(),
    };

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
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("megan");

    // Test multiple surface planes
    let surfaces = vec![
        "Application",
        "Administration",
        "Cluster",
        "BackupAgent",
        "MonitoringAgent",
    ];

    for surface in surfaces {
        let event = AdmissionAuditEvent::ProcedureDispatchAuthorized {
            trace_id,
            procedure_id: proc_id,
            surface_plane: surface.to_string(),
            certificate_identity: "CN=test-user".to_string(),
            affected_principal: principal.clone(),
            dispatch_timestamp: now(),
        };

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

// ============================================================================
// Cross-Cutting Contract Tests
// ============================================================================

#[test]
fn all_admission_events_carry_non_zero_trace_ids() {
    let valid_trace = test_trace_id();
    let proc_id = test_procedure_id();
    let principal = test_principal("noah");

    let events: Vec<AdmissionAuditEvent> = vec![
        AdmissionAuditEvent::ContractValidated {
            trace_id: valid_trace,
            procedure_id: proc_id,
            input_row_count: 100,
            result: ContractValidationResult::Valid,
            validation_details: "valid".to_string(),
            affected_principal: principal.clone(),
            event_timestamp: now(),
        },
        AdmissionAuditEvent::AdmissionDecision {
            trace_id: valid_trace,
            procedure_id: proc_id,
            decision: AdmissionDecisionKind::Accepted,
            reason: "accepted".to_string(),
            affected_principal: principal.clone(),
            event_timestamp: now(),
        },
        AdmissionAuditEvent::RequestThrottled {
            trace_id: valid_trace,
            procedure_id: proc_id,
            backpressure_reason: BackpressureReason::BufferPoolFull,
            retry_after_ms: 100,
            event_timestamp: now(),
        },
    ];

    for event in events {
        assert!(!event.trace_id().is_zero());
    }
}

#[test]
fn admission_event_principal_binding_is_immutable() {
    // Principal is created once and bound immutably to the event
    let principal = test_principal("olivia");
    let original_principal_id = principal.principal_id.clone();

    let event = AdmissionAuditEvent::AdmissionDecision {
        trace_id: test_trace_id(),
        procedure_id: test_procedure_id(),
        decision: AdmissionDecisionKind::Accepted,
        reason: "test".to_string(),
        affected_principal: principal,
        event_timestamp: now(),
    };

    // Principal cannot be re-bound after event creation (Rust ownership guarantees this)
    assert_eq!(event.affected_principal_id(), original_principal_id);

    // Principal binding persists across method calls
    let id1 = event.affected_principal_id();
    let id2 = event.affected_principal_id();
    assert_eq!(id1, id2);
    assert_eq!(id1, original_principal_id);
}

#[test]
fn admission_audit_event_type_labels_are_unique() {
    let trace = test_trace_id();
    let proc = test_procedure_id();
    let principal = test_principal("paul");

    let contract_validated = AdmissionAuditEvent::ContractValidated {
        trace_id: trace,
        procedure_id: proc,
        input_row_count: 0,
        result: ContractValidationResult::Valid,
        validation_details: "test".to_string(),
        affected_principal: principal.clone(),
        event_timestamp: now(),
    };

    let admission_decision = AdmissionAuditEvent::AdmissionDecision {
        trace_id: trace,
        procedure_id: proc,
        decision: AdmissionDecisionKind::Accepted,
        reason: "test".to_string(),
        affected_principal: principal.clone(),
        event_timestamp: now(),
    };

    let permission_failed = AdmissionAuditEvent::PermissionCheckFailed {
        trace_id: trace,
        procedure_id: proc,
        affected_principal: principal.clone(),
        required_permission: "test".to_string(),
        actual_permission_set: "".to_string(),
        event_timestamp: now(),
    };

    let throttled = AdmissionAuditEvent::RequestThrottled {
        trace_id: trace,
        procedure_id: proc,
        backpressure_reason: BackpressureReason::BufferPoolFull,
        retry_after_ms: 0,
        event_timestamp: now(),
    };

    let authorized = AdmissionAuditEvent::ProcedureDispatchAuthorized {
        trace_id: trace,
        procedure_id: proc,
        surface_plane: "test".to_string(),
        certificate_identity: "test".to_string(),
        affected_principal: principal,
        dispatch_timestamp: now(),
    };

    let labels = [
        contract_validated.type_label(),
        admission_decision.type_label(),
        permission_failed.type_label(),
        throttled.type_label(),
        authorized.type_label(),
    ];

    // All labels must be unique (no duplicates)
    assert_eq!(
        labels.len(),
        labels
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    );

    // Verify explicit values
    assert_eq!(contract_validated.type_label(), "contract_validated");
    assert_eq!(admission_decision.type_label(), "admission_decision");
    assert_eq!(permission_failed.type_label(), "permission_check_failed");
    assert_eq!(throttled.type_label(), "request_throttled");
    assert_eq!(authorized.type_label(), "procedure_dispatch_authorized");
}

#[test]
fn admission_events_validation_requires_all_fields() {
    // Valid event
    let valid_event = AdmissionAuditEvent::ContractValidated {
        trace_id: test_trace_id(),
        procedure_id: test_procedure_id(),
        input_row_count: 100,
        result: ContractValidationResult::Valid,
        validation_details: "test".to_string(),
        affected_principal: test_principal("quinn"),
        event_timestamp: now(),
    };
    assert!(valid_event.is_valid());

    // Invalid due to empty validation_details
    let invalid_event = AdmissionAuditEvent::ContractValidated {
        trace_id: test_trace_id(),
        procedure_id: test_procedure_id(),
        input_row_count: 100,
        result: ContractValidationResult::Valid,
        validation_details: "".to_string(),
        affected_principal: test_principal("rachel"),
        event_timestamp: now(),
    };
    assert!(!invalid_event.is_valid());

    // Invalid due to zero trace_id
    let invalid_trace = AdmissionAuditEvent::ContractValidated {
        trace_id: TraceId::new(0),
        procedure_id: test_procedure_id(),
        input_row_count: 100,
        result: ContractValidationResult::Valid,
        validation_details: "test".to_string(),
        affected_principal: test_principal("sam"),
        event_timestamp: now(),
    };
    assert!(!invalid_trace.is_valid());

    // Invalid due to empty principal
    let invalid_principal = AdmissionAuditEvent::AdmissionDecision {
        trace_id: test_trace_id(),
        procedure_id: test_procedure_id(),
        decision: AdmissionDecisionKind::Accepted,
        reason: "test".to_string(),
        affected_principal: AffectedPrincipal::new(""),
        event_timestamp: now(),
    };
    assert!(!invalid_principal.is_valid());
}
