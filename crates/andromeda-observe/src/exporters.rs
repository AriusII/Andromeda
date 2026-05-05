//! Observability exporter trait boundary and configurations.
//!
//! This module defines the trait interfaces for observability data exporters,
//! enabling multiple exporters to run simultaneously with a unified contract.
//!
//! ## Contract Overview
//!
//! The Exporter trait defines three primary operations:
//! 1. **Single Trace Export**: Export individual decision traces
//! 2. **Single Metric Export**: Export individual metrics
//! 3. **Batch Export**: Efficiently export multiple traces and metrics together
//!
//! ### Key Invariants
//!
//! - **Unified Interface**: All exporters implement the same trait
//! - **Concurrent Execution**: Multiple exporters can run simultaneously
//! - **Error Observability**: All failures returned via `AndromedaResult<T>`
//! - **Backend Agnostic**: Supports OpenTelemetry, Prometheus, local files, etc.
//! - **Configuration Validation**: All configs validated before use
//! - **Timeout Guarantees**: Exporters respect configured timeouts
//! - **Retry Policies**: Configurable retry behavior per exporter
//!
//! ## Supported Backends
//!
//! - **OpenTelemetry**: Sends traces and metrics to OTel collectors
//! - **Prometheus**: Exports metrics to Prometheus remote write endpoints
//! - **Local File**: Writes events to local files for forensic analysis
//! - **Syslog**: Emits events to syslog for centralized logging
//! - **Null**: Discards all data (for testing)
//!
//! ## Usage Pattern
//!
//! ```ignore
//! use andromeda_observe::exporters::{ExporterTrait, ExporterConfig, ExporterBackend};
//! use andromeda_core::AndromedaResult;
//!
//! let config = ExporterConfig {
//!     backend: ExporterBackend::OpenTelemetry,
//!     endpoint_url: "http://localhost:4317".to_string(),
//!     batch_size: 100,
//!     timeout_ms: 5000,
//!     retry_policy: RetryPolicy::ExponentialBackoff {
//!         max_retries: 3,
//!         initial_delay_ms: 100,
//!     },
//! };
//!
//! let exporter = create_exporter(&config)?;
//! exporter.export_trace(trace)?;
//! exporter.batch_export(traces, metrics)?;
//! ```

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::fmt;

/// A decision trace record for export.
///
/// Contains the execution decision and trace metadata needed for analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportDecisionTrace {
    /// Unique trace ID for correlation
    pub trace_id: String,
    /// Human-readable trace name or operation
    pub operation: String,
    /// Decision outcome (e.g., "allowed", "denied", "error")
    pub decision: String,
    /// Structured trace data (JSON-formatted)
    pub payload: String,
    /// Timestamp of the trace (milliseconds since epoch)
    pub timestamp_ms: u64,
    /// Trace attributes for filtering and aggregation
    pub attributes: std::collections::HashMap<String, String>,
}

impl ExportDecisionTrace {
    /// Validate the trace structure.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "trace id must not be empty",
            ));
        }

        if self.operation.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "operation must not be empty",
            ));
        }

        if self.decision.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "decision must not be empty",
            ));
        }

        if self.timestamp_ms == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "timestamp must not be zero",
            ));
        }

        Ok(())
    }
}

/// A metric for export.
///
/// Represents a time-series measurement with value and labels.
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    /// Metric name (e.g., "procedure_execution_time_ms")
    pub name: String,
    /// Metric value (can be integer or floating-point)
    pub value: f64,
    /// Metric unit (e.g., "ms", "bytes", "count")
    pub unit: String,
    /// Timestamp of the measurement (milliseconds since epoch)
    pub timestamp_ms: u64,
    /// Labels for filtering and aggregation
    pub labels: std::collections::HashMap<String, String>,
}

impl Metric {
    /// Validate the metric structure.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "metric name must not be empty",
            ));
        }

        if self.unit.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "metric unit must not be empty",
            ));
        }

        if self.timestamp_ms == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "timestamp must not be zero",
            ));
        }

        if !self.value.is_finite() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "metric value must be finite",
            ));
        }

        Ok(())
    }
}

/// Exporter backend types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExporterBackend {
    /// OpenTelemetry protocol endpoint
    OpenTelemetry,
    /// Prometheus remote write endpoint
    Prometheus,
    /// Local file sink
    LocalFile,
    /// Syslog sink
    Syslog,
    /// Null sink (discards all data)
    Null,
}

impl fmt::Display for ExporterBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenTelemetry => write!(f, "OpenTelemetry"),
            Self::Prometheus => write!(f, "Prometheus"),
            Self::LocalFile => write!(f, "LocalFile"),
            Self::Syslog => write!(f, "Syslog"),
            Self::Null => write!(f, "Null"),
        }
    }
}

/// Retry policy for failed exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryPolicy {
    /// No retry on failure
    NoRetry,
    /// Fixed delay between retries
    FixedDelay { max_retries: u32, delay_ms: u32 },
    /// Exponential backoff with jitter
    ExponentialBackoff {
        max_retries: u32,
        initial_delay_ms: u32,
    },
}

impl RetryPolicy {
    /// Validate the retry policy.
    pub fn validate(self) -> AndromedaResult<()> {
        match self {
            Self::FixedDelay {
                max_retries,
                delay_ms,
            } => {
                if delay_ms == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "delay_ms must not be zero",
                    ));
                }
                if max_retries > 100 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Resource,
                        "max_retries must not exceed 100",
                    ));
                }
                Ok(())
            }
            Self::ExponentialBackoff {
                max_retries,
                initial_delay_ms,
            } => {
                if initial_delay_ms == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "initial_delay_ms must not be zero",
                    ));
                }
                if max_retries > 100 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Resource,
                        "max_retries must not exceed 100",
                    ));
                }
                Ok(())
            }
            Self::NoRetry => Ok(()),
        }
    }
}

/// Configuration for an exporter instance.
#[derive(Debug, Clone, PartialEq)]
pub struct ExporterConfig {
    /// The backend type
    pub backend: ExporterBackend,
    /// The endpoint URL (used by network-based backends)
    pub endpoint_url: String,
    /// Maximum batch size for batch exports
    pub batch_size: u32,
    /// Timeout for individual exports (in milliseconds)
    pub timeout_ms: u32,
    /// Retry policy for failed exports
    pub retry_policy: RetryPolicy,
}

impl ExporterConfig {
    /// Validate the exporter configuration.
    pub fn validate(&self) -> AndromedaResult<()> {
        match self.backend {
            ExporterBackend::OpenTelemetry
            | ExporterBackend::Prometheus
            | ExporterBackend::Syslog => {
                if self.endpoint_url.is_empty() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "endpoint_url must not be empty for network backends",
                    ));
                }
                // Validate URL format (basic check)
                if !self.endpoint_url.starts_with("http://")
                    && !self.endpoint_url.starts_with("https://")
                    && !self.endpoint_url.starts_with("unix://")
                {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "endpoint_url must be a valid URL",
                    ));
                }
            }
            ExporterBackend::LocalFile => {
                if self.endpoint_url.is_empty() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "endpoint_url must be a file path for LocalFile backend",
                    ));
                }
            }
            ExporterBackend::Null => {
                // Null backend has no specific requirements
            }
        }

        if self.batch_size == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "batch_size must not be zero",
            ));
        }

        if self.batch_size > 100000 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "batch_size must not exceed 100000",
            ));
        }

        if self.timeout_ms == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "timeout_ms must not be zero",
            ));
        }

        if self.timeout_ms > 300000 {
            // 5 minutes max
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "timeout_ms must not exceed 300000 (5 minutes)",
            ));
        }

        self.retry_policy.validate()?;

        Ok(())
    }
}

/// Exporter trait defining the boundary contract for observability data.
///
/// All observability exporters must implement this trait.
/// Multiple exporters can be instantiated and run concurrently.
pub trait ExporterTrait: Send + Sync {
    /// Export a single decision trace.
    ///
    /// This is a non-batched operation suitable for low-frequency events.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The trace is invalid
    /// - The export operation times out
    /// - The backend is unreachable or returns an error
    fn export_trace(&self, trace: ExportDecisionTrace) -> AndromedaResult<()>;

    /// Export a single metric.
    ///
    /// This is a non-batched operation suitable for low-frequency metrics.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The metric is invalid
    /// - The export operation times out
    /// - The backend is unreachable or returns an error
    fn export_metric(&self, metric: Metric) -> AndromedaResult<()>;

    /// Export a batch of traces and metrics together.
    ///
    /// This is more efficient than exporting individually when dealing with
    /// large volumes of observability data.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Any trace or metric is invalid
    /// - The batch export operation times out
    /// - The backend is unreachable or returns an error
    fn batch_export(
        &self,
        traces: Vec<ExportDecisionTrace>,
        metrics: Vec<Metric>,
    ) -> AndromedaResult<()>;

    /// Get the configuration of this exporter.
    fn config(&self) -> &ExporterConfig;

    /// Check if the exporter is currently healthy and operational.
    fn is_healthy(&self) -> bool;

    /// Get the total number of successful exports.
    fn successful_exports(&self) -> u64;

    /// Get the total number of failed exports.
    fn failed_exports(&self) -> u64;
}

/// Mock implementation of `ExporterTrait` for testing.
#[derive(Debug, Clone)]
pub struct MockExporter {
    config: ExporterConfig,
    traces: std::sync::Arc<std::sync::Mutex<Vec<ExportDecisionTrace>>>,
    metrics: std::sync::Arc<std::sync::Mutex<Vec<Metric>>>,
    successful_count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    failed_count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    healthy: std::sync::Arc<std::sync::Mutex<bool>>,
}

impl MockExporter {
    /// Create a new mock exporter with the given configuration.
    pub fn new(config: ExporterConfig) -> AndromedaResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            traces: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            metrics: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            successful_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            failed_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            healthy: std::sync::Arc::new(std::sync::Mutex::new(true)),
        })
    }

    /// Get a copy of all exported traces.
    pub fn get_traces(&self) -> Vec<ExportDecisionTrace> {
        self.traces.lock().unwrap().clone()
    }

    /// Get a copy of all exported metrics.
    pub fn get_metrics(&self) -> Vec<Metric> {
        self.metrics.lock().unwrap().clone()
    }

    /// Simulate a failure (set health to false).
    pub fn simulate_failure(&self) {
        *self.healthy.lock().unwrap() = false;
    }

    /// Restore health.
    pub fn restore_health(&self) {
        *self.healthy.lock().unwrap() = true;
    }
}

impl ExporterTrait for MockExporter {
    fn export_trace(&self, trace: ExportDecisionTrace) -> AndromedaResult<()> {
        trace.validate()?;

        if !*self.healthy.lock().unwrap() {
            self.failed_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "exporter is not healthy",
            ));
        }

        self.traces.lock().unwrap().push(trace);
        self.successful_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn export_metric(&self, metric: Metric) -> AndromedaResult<()> {
        metric.validate()?;

        if !*self.healthy.lock().unwrap() {
            self.failed_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "exporter is not healthy",
            ));
        }

        self.metrics.lock().unwrap().push(metric);
        self.successful_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn batch_export(
        &self,
        traces: Vec<ExportDecisionTrace>,
        metrics: Vec<Metric>,
    ) -> AndromedaResult<()> {
        // Validate all inputs
        for trace in &traces {
            trace.validate()?;
        }
        for metric in &metrics {
            metric.validate()?;
        }

        if !*self.healthy.lock().unwrap() {
            let count = (traces.len() + metrics.len()) as u64;
            self.failed_count
                .fetch_add(count, std::sync::atomic::Ordering::SeqCst);
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "exporter is not healthy",
            ));
        }

        // Store all items
        self.traces.lock().unwrap().extend(traces.clone());
        self.metrics.lock().unwrap().extend(metrics.clone());

        let count = (traces.len() + metrics.len()) as u64;
        self.successful_count
            .fetch_add(count, std::sync::atomic::Ordering::SeqCst);

        Ok(())
    }

    fn config(&self) -> &ExporterConfig {
        &self.config
    }

    fn is_healthy(&self) -> bool {
        *self.healthy.lock().unwrap()
    }

    fn successful_exports(&self) -> u64 {
        self.successful_count
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn failed_exports(&self) -> u64 {
        self.failed_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_trace_validation() {
        // Valid trace
        let valid = ExportDecisionTrace {
            trace_id: "trace-001".to_string(),
            operation: "execute_procedure".to_string(),
            decision: "allowed".to_string(),
            payload: "{}".to_string(),
            timestamp_ms: 1000,
            attributes: std::collections::HashMap::new(),
        };
        assert!(valid.validate().is_ok());

        // Invalid: empty trace_id
        let invalid_id = ExportDecisionTrace {
            trace_id: String::new(),
            ..valid.clone()
        };
        assert!(invalid_id.validate().is_err());

        // Invalid: empty operation
        let invalid_op = ExportDecisionTrace {
            operation: String::new(),
            ..valid.clone()
        };
        assert!(invalid_op.validate().is_err());

        // Invalid: empty decision
        let invalid_decision = ExportDecisionTrace {
            decision: String::new(),
            ..valid.clone()
        };
        assert!(invalid_decision.validate().is_err());

        // Invalid: zero timestamp
        let invalid_ts = ExportDecisionTrace {
            timestamp_ms: 0,
            ..valid.clone()
        };
        assert!(invalid_ts.validate().is_err());
    }

    #[test]
    fn test_metric_validation() {
        // Valid metric
        let valid = Metric {
            name: "procedure_time".to_string(),
            value: 42.5,
            unit: "ms".to_string(),
            timestamp_ms: 1000,
            labels: std::collections::HashMap::new(),
        };
        assert!(valid.validate().is_ok());

        // Invalid: empty name
        let invalid_name = Metric {
            name: String::new(),
            ..valid.clone()
        };
        assert!(invalid_name.validate().is_err());

        // Invalid: empty unit
        let invalid_unit = Metric {
            unit: String::new(),
            ..valid.clone()
        };
        assert!(invalid_unit.validate().is_err());

        // Invalid: zero timestamp
        let invalid_ts = Metric {
            timestamp_ms: 0,
            ..valid.clone()
        };
        assert!(invalid_ts.validate().is_err());

        // Invalid: NaN value
        let invalid_nan = Metric {
            value: f64::NAN,
            ..valid.clone()
        };
        assert!(invalid_nan.validate().is_err());

        // Invalid: infinite value
        let invalid_inf = Metric {
            value: f64::INFINITY,
            ..valid.clone()
        };
        assert!(invalid_inf.validate().is_err());
    }

    #[test]
    fn test_exporter_config_validation() {
        // Valid OpenTelemetry config
        let valid_otel = ExporterConfig {
            backend: ExporterBackend::OpenTelemetry,
            endpoint_url: "http://localhost:4317".to_string(),
            batch_size: 100,
            timeout_ms: 5000,
            retry_policy: RetryPolicy::ExponentialBackoff {
                max_retries: 3,
                initial_delay_ms: 100,
            },
        };
        assert!(valid_otel.validate().is_ok());

        // Invalid: missing endpoint for network backend
        let invalid_no_endpoint = ExporterConfig {
            endpoint_url: String::new(),
            ..valid_otel.clone()
        };
        assert!(invalid_no_endpoint.validate().is_err());

        // Invalid: zero batch size
        let invalid_batch = ExporterConfig {
            batch_size: 0,
            ..valid_otel.clone()
        };
        assert!(invalid_batch.validate().is_err());

        // Invalid: batch size too large
        let invalid_batch_large = ExporterConfig {
            batch_size: 200000,
            ..valid_otel.clone()
        };
        assert!(invalid_batch_large.validate().is_err());

        // Invalid: zero timeout
        let invalid_timeout = ExporterConfig {
            timeout_ms: 0,
            ..valid_otel.clone()
        };
        assert!(invalid_timeout.validate().is_err());

        // Invalid: timeout too large
        let invalid_timeout_large = ExporterConfig {
            timeout_ms: 500000,
            ..valid_otel.clone()
        };
        assert!(invalid_timeout_large.validate().is_err());
    }

    #[test]
    fn test_retry_policy_validation() {
        // Valid fixed delay
        let valid_fixed = RetryPolicy::FixedDelay {
            max_retries: 3,
            delay_ms: 100,
        };
        assert!(valid_fixed.validate().is_ok());

        // Invalid fixed delay: zero delay
        let invalid_fixed = RetryPolicy::FixedDelay {
            max_retries: 3,
            delay_ms: 0,
        };
        assert!(invalid_fixed.validate().is_err());

        // Invalid fixed delay: too many retries
        let invalid_fixed_retries = RetryPolicy::FixedDelay {
            max_retries: 200,
            delay_ms: 100,
        };
        assert!(invalid_fixed_retries.validate().is_err());

        // Valid exponential backoff
        let valid_exp = RetryPolicy::ExponentialBackoff {
            max_retries: 5,
            initial_delay_ms: 50,
        };
        assert!(valid_exp.validate().is_ok());

        // Invalid exponential: zero initial delay
        let invalid_exp = RetryPolicy::ExponentialBackoff {
            max_retries: 5,
            initial_delay_ms: 0,
        };
        assert!(invalid_exp.validate().is_err());

        // Valid no-retry
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

        // Simulate failure
        exporter.simulate_failure();
        assert!(!exporter.is_healthy());

        // Try to export (should fail)
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

        // Restore health
        exporter.restore_health();
        assert!(exporter.is_healthy());

        // Export again (should succeed)
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
}
