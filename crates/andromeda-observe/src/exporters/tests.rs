use super::*;

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
