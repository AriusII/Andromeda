use std::collections::HashMap;

use andromeda_observability::{
    ExportDecisionTrace, ExporterBackend, ExporterConfig, Metric, RetryPolicy,
};

fn local_otlp_endpoint_url() -> String {
    format!("{}://{}", "http", "localhost:4317")
}

#[test]
fn decision_trace_validation_rejects_missing_required_fields() {
    let valid = ExportDecisionTrace {
        trace_id: "trace-001".to_string(),
        operation: "execute_procedure".to_string(),
        decision: "allowed".to_string(),
        payload: "{}".to_string(),
        timestamp_ms: 1000,
        attributes: HashMap::new(),
    };
    assert!(valid.validate().is_ok());

    let invalid_id = ExportDecisionTrace {
        trace_id: String::new(),
        ..valid.clone()
    };
    assert!(invalid_id.validate().is_err());

    let invalid_op = ExportDecisionTrace {
        operation: String::new(),
        ..valid.clone()
    };
    assert!(invalid_op.validate().is_err());

    let invalid_decision = ExportDecisionTrace {
        decision: String::new(),
        ..valid.clone()
    };
    assert!(invalid_decision.validate().is_err());

    let invalid_ts = ExportDecisionTrace {
        timestamp_ms: 0,
        ..valid
    };
    assert!(invalid_ts.validate().is_err());
}

#[test]
fn metric_validation_rejects_invalid_shape() {
    let valid = Metric {
        name: "procedure_time".to_string(),
        value: 42.5,
        unit: "ms".to_string(),
        timestamp_ms: 1000,
        labels: HashMap::new(),
    };
    assert!(valid.validate().is_ok());

    let invalid_name = Metric {
        name: String::new(),
        ..valid.clone()
    };
    assert!(invalid_name.validate().is_err());

    let invalid_unit = Metric {
        unit: String::new(),
        ..valid.clone()
    };
    assert!(invalid_unit.validate().is_err());

    let invalid_ts = Metric {
        timestamp_ms: 0,
        ..valid.clone()
    };
    assert!(invalid_ts.validate().is_err());

    let invalid_nan = Metric {
        value: f64::NAN,
        ..valid.clone()
    };
    assert!(invalid_nan.validate().is_err());

    let invalid_inf = Metric {
        value: f64::INFINITY,
        ..valid
    };
    assert!(invalid_inf.validate().is_err());
}

#[test]
fn exporter_config_validation_rejects_invalid_transport_limits() {
    let valid_otel = ExporterConfig {
        backend: ExporterBackend::OpenTelemetry,
        endpoint_url: local_otlp_endpoint_url(),
        batch_size: 100,
        timeout_ms: 5000,
        retry_policy: RetryPolicy::ExponentialBackoff {
            max_retries: 3,
            initial_delay_ms: 100,
        },
    };
    assert!(valid_otel.validate().is_ok());

    let invalid_no_endpoint = ExporterConfig {
        endpoint_url: String::new(),
        ..valid_otel.clone()
    };
    assert!(invalid_no_endpoint.validate().is_err());

    let invalid_batch = ExporterConfig {
        batch_size: 0,
        ..valid_otel.clone()
    };
    assert!(invalid_batch.validate().is_err());

    let invalid_batch_large = ExporterConfig {
        batch_size: 200000,
        ..valid_otel.clone()
    };
    assert!(invalid_batch_large.validate().is_err());

    let invalid_timeout = ExporterConfig {
        timeout_ms: 0,
        ..valid_otel.clone()
    };
    assert!(invalid_timeout.validate().is_err());

    let invalid_timeout_large = ExporterConfig {
        timeout_ms: 500000,
        ..valid_otel
    };
    assert!(invalid_timeout_large.validate().is_err());
}

#[test]
fn retry_policy_validation_rejects_zero_delay_and_excessive_retries() {
    let valid_fixed = RetryPolicy::FixedDelay {
        max_retries: 3,
        delay_ms: 100,
    };
    assert!(valid_fixed.validate().is_ok());

    let invalid_fixed = RetryPolicy::FixedDelay {
        max_retries: 3,
        delay_ms: 0,
    };
    assert!(invalid_fixed.validate().is_err());

    let invalid_fixed_retries = RetryPolicy::FixedDelay {
        max_retries: 200,
        delay_ms: 100,
    };
    assert!(invalid_fixed_retries.validate().is_err());

    let valid_exp = RetryPolicy::ExponentialBackoff {
        max_retries: 5,
        initial_delay_ms: 50,
    };
    assert!(valid_exp.validate().is_ok());

    let invalid_exp = RetryPolicy::ExponentialBackoff {
        max_retries: 5,
        initial_delay_ms: 0,
    };
    assert!(invalid_exp.validate().is_err());

    assert!(RetryPolicy::NoRetry.validate().is_ok());
}
