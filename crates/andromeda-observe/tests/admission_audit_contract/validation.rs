use crate::support::*;

#[test]
fn all_admission_events_carry_non_zero_trace_ids() {
    let events: Vec<AdmissionAuditEvent> = vec![
        contract_validated_event("noah", 100, ContractValidationResult::Valid, "valid"),
        admission_decision_event("noah", AdmissionDecisionKind::Accepted, "accepted"),
        request_throttled_event(BackpressureReason::BufferPoolFull, 100),
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
    let contract_validated =
        contract_validated_event("paul", 0, ContractValidationResult::Valid, "test");
    let admission_decision =
        admission_decision_event("paul", AdmissionDecisionKind::Accepted, "test");
    let permission_failed = permission_check_failed_event("paul", "test", "");
    let throttled = request_throttled_event(BackpressureReason::BufferPoolFull, 0);
    let authorized = procedure_dispatch_authorized_event("paul", "test", "test");

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
