use super::*;

#[test]
fn backpressure_retry_metadata_is_bounded_and_structured() {
    let signal = BackpressureSignal {
        reason: BackpressureReason::ExecutionQueueSaturated,
        request_id: Some(RequestId::new(501)),
        retry_after_millis: Some(250),
    };
    signal.validate_retry_policy().unwrap();

    let missing_request_scope = BackpressureSignal {
        request_id: None,
        ..signal
    };
    assert_eq!(
        missing_request_scope
            .validate_retry_policy()
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Resource
    );

    let error = ErrorEnvelope {
        request_id: signal.request_id,
        session_id: Some(SessionId::new(601)),
        trace_id: Some("trace".to_string()),
        family: ErrorFamily::Resource,
        code: "BACKPRESSURE".to_string(),
        message: "execution queue saturated".to_string(),
        transaction_effect: TransactionEffect::NoTransaction,
        retry_disposition: RetryDisposition::Backpressure,
        retry_after_ms: None,
        backpressure: Some(BackpressureMetadata {
            retry_after_ms: signal.retry_after_millis,
            capacity_percent: Some(75),
            shed_load: true,
        }),
    };
    error.validate().unwrap();

    let malformed_error = ErrorEnvelope {
        backpressure: Some(BackpressureMetadata {
            capacity_percent: Some(101),
            ..error.backpressure.unwrap()
        }),
        ..error
    };
    assert_eq!(
        malformed_error.validate().unwrap_err().kind(),
        AndromedaErrorKind::Resource
    );
}
