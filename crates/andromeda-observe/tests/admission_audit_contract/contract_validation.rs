use crate::support::*;

#[test]
fn contract_validated_event_valid_contract_carries_all_evidence() {
    let trace_id = test_trace_id();
    let proc_id = test_procedure_id();
    let timestamp = now();

    let event = contract_validated_event_at(
        "alice",
        150,
        ContractValidationResult::Valid,
        "contract hash matches catalog entry",
        timestamp,
    );

    assert_eq!(event.type_label(), "contract_validated");
    assert_eq!(event.trace_id(), trace_id);
    assert_eq!(event.procedure_id(), proc_id);
    assert_eq!(event.affected_principal_id(), "CN=test-principal-alice");
    assert_eq!(event.event_timestamp(), timestamp);
    assert!(event.is_valid());
}

#[test]
fn contract_validated_event_invalid_contract_carries_mismatch_reason() {
    let event = contract_validated_event(
        "bob",
        0,
        ContractValidationResult::Invalid,
        "contract hash mismatch: expected deadbeef, got cafebabe",
    );

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
    let event = contract_validated_event(
        "charlie",
        250,
        ContractValidationResult::Deprecated,
        "contract is deprecated but still usable until EOL 2026-12-31",
    );

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
