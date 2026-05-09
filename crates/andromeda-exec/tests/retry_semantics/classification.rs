use crate::support::{PERSISTENT_KINDS, TRANSIENT_KINDS};
use andromeda_error::AndromedaErrorKind;
use andromeda_retry::ErrorRetryability;

#[test]
fn test_error_classification_transient_types() {
    for kind in TRANSIENT_KINDS {
        let retryability = ErrorRetryability::classify(kind);
        assert_eq!(
            retryability,
            ErrorRetryability::Transient,
            "expected {:?} to be transient",
            kind
        );
        assert!(retryability.is_transient());
    }
}

#[test]
fn test_error_classification_persistent_types() {
    for kind in PERSISTENT_KINDS {
        let retryability = ErrorRetryability::classify(kind);
        assert_eq!(
            retryability,
            ErrorRetryability::Persistent,
            "expected {:?} to be persistent",
            kind
        );
        assert!(retryability.is_persistent());
    }
}

#[test]
fn test_security_and_contract_errors_fail_immediately() {
    let security_retryability = ErrorRetryability::classify(AndromedaErrorKind::Security);
    assert_eq!(security_retryability, ErrorRetryability::Persistent);
    assert!(!security_retryability.is_transient());

    let contract_retryability = ErrorRetryability::classify(AndromedaErrorKind::Contract);
    assert_eq!(contract_retryability, ErrorRetryability::Persistent);
}

#[test]
fn test_transport_resource_and_protocol_errors_are_retryable() {
    for kind in [
        AndromedaErrorKind::Transport,
        AndromedaErrorKind::Resource,
        AndromedaErrorKind::Protocol,
    ] {
        let retryability = ErrorRetryability::classify(kind);
        assert_eq!(retryability, ErrorRetryability::Transient);
        assert!(retryability.is_transient());
    }
}
