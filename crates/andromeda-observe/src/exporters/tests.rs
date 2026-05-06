use super::*;

fn local_otlp_endpoint_url() -> String {
    format!("{}://{}", "http", "localhost:4317")
}

#[test]
fn test_decision_trace_validation() {
    let valid = ExportDecisionTrace {
        trace_id: "trace-001".to_string(),
        operation: "execute_procedure".to_string(),
        decision: "allowed".to_string(),
        payload: "{}".to_string(),
        timestamp_ms: 1000,
        attributes: std::collections::HashMap::new(),
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
        ..valid.clone()
    };
    assert!(invalid_ts.validate().is_err());
}

#[test]
fn test_metric_validation() {
    let valid = Metric {
        name: "procedure_time".to_string(),
        value: 42.5,
        unit: "ms".to_string(),
        timestamp_ms: 1000,
        labels: std::collections::HashMap::new(),
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
        ..valid.clone()
    };
    assert!(invalid_inf.validate().is_err());
}

#[test]
fn test_exporter_config_validation() {
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
        ..valid_otel.clone()
    };
    assert!(invalid_timeout_large.validate().is_err());
}

#[test]
fn test_retry_policy_validation() {
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

#[test]
fn test_mock_exporter_single_trace_export() {
    let config = ExporterConfig {
        backend: ExporterBackend::Null,
        endpoint_url: "null".to_string(),
        batch_size: 100,
        timeout_ms: 5000,
        retry_policy: RetryPolicy::NoRetry,
    };

    let exporter = MockExporter::new(config).unwrap();
    assert!(exporter.is_healthy());

    let trace = ExportDecisionTrace {
        trace_id: "trace-001".to_string(),
        operation: "test_op".to_string(),
        decision: "allowed".to_string(),
        payload: "{}".to_string(),
        timestamp_ms: 1000,
        attributes: std::collections::HashMap::new(),
    };

    assert!(exporter.export_trace(trace).is_ok());
    assert_eq!(exporter.successful_exports(), 1);
    assert_eq!(exporter.failed_exports(), 0);
    assert_eq!(exporter.get_traces().len(), 1);
}

#[test]
fn test_mock_exporter_single_metric_export() {
    let config = ExporterConfig {
        backend: ExporterBackend::Null,
        endpoint_url: "null".to_string(),
        batch_size: 100,
        timeout_ms: 5000,
        retry_policy: RetryPolicy::NoRetry,
    };

    let exporter = MockExporter::new(config).unwrap();

    let metric = Metric {
        name: "test_metric".to_string(),
        value: 42.0,
        unit: "ms".to_string(),
        timestamp_ms: 1000,
        labels: std::collections::HashMap::new(),
    };

    assert!(exporter.export_metric(metric).is_ok());
    assert_eq!(exporter.successful_exports(), 1);
    assert_eq!(exporter.failed_exports(), 0);
    assert_eq!(exporter.get_metrics().len(), 1);
}

#[test]
fn test_mock_exporter_batch_export() {
    let config = ExporterConfig {
        backend: ExporterBackend::Null,
        endpoint_url: "null".to_string(),
        batch_size: 100,
        timeout_ms: 5000,
        retry_policy: RetryPolicy::NoRetry,
    };

    let exporter = MockExporter::new(config).unwrap();

    let traces = vec![
        ExportDecisionTrace {
            trace_id: "trace-001".to_string(),
            operation: "op1".to_string(),
            decision: "allowed".to_string(),
            payload: "{}".to_string(),
            timestamp_ms: 1000,
            attributes: std::collections::HashMap::new(),
        },
        ExportDecisionTrace {
            trace_id: "trace-002".to_string(),
            operation: "op2".to_string(),
            decision: "denied".to_string(),
            payload: "{}".to_string(),
            timestamp_ms: 2000,
            attributes: std::collections::HashMap::new(),
        },
    ];

    let metrics = vec![
        Metric {
            name: "metric1".to_string(),
            value: 10.0,
            unit: "ms".to_string(),
            timestamp_ms: 1000,
            labels: std::collections::HashMap::new(),
        },
        Metric {
            name: "metric2".to_string(),
            value: 20.0,
            unit: "bytes".to_string(),
            timestamp_ms: 2000,
            labels: std::collections::HashMap::new(),
        },
    ];

    assert!(exporter.batch_export(traces, metrics).is_ok());
    assert_eq!(exporter.successful_exports(), 4); // 2 traces + 2 metrics
    assert_eq!(exporter.failed_exports(), 0);
    assert_eq!(exporter.get_traces().len(), 2);
    assert_eq!(exporter.get_metrics().len(), 2);
}

#[test]
fn test_mock_exporter_failure_tracking() {
    let config = ExporterConfig {
        backend: ExporterBackend::Null,
        endpoint_url: "null".to_string(),
        batch_size: 100,
        timeout_ms: 5000,
        retry_policy: RetryPolicy::NoRetry,
    };

    let exporter = MockExporter::new(config).unwrap();

    // Export successfully
    let trace = ExportDecisionTrace {
        trace_id: "trace-001".to_string(),
        operation: "op".to_string(),
        decision: "allowed".to_string(),
        payload: "{}".to_string(),
        timestamp_ms: 1000,
        attributes: std::collections::HashMap::new(),
    };
    assert!(exporter.export_trace(trace).is_ok());

    exporter.simulate_failure();
    assert!(!exporter.is_healthy());

    let trace2 = ExportDecisionTrace {
        trace_id: "trace-002".to_string(),
        operation: "op".to_string(),
        decision: "allowed".to_string(),
        payload: "{}".to_string(),
        timestamp_ms: 2000,
        attributes: std::collections::HashMap::new(),
    };
    assert!(exporter.export_trace(trace2).is_err());

    assert_eq!(exporter.successful_exports(), 1);
    assert_eq!(exporter.failed_exports(), 1);

    exporter.restore_health();
    assert!(exporter.is_healthy());

    let trace3 = ExportDecisionTrace {
        trace_id: "trace-003".to_string(),
        operation: "op".to_string(),
        decision: "allowed".to_string(),
        payload: "{}".to_string(),
        timestamp_ms: 3000,
        attributes: std::collections::HashMap::new(),
    };
    assert!(exporter.export_trace(trace3).is_ok());

    assert_eq!(exporter.successful_exports(), 2);
    assert_eq!(exporter.failed_exports(), 1);
}
