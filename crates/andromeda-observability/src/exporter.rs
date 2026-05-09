use std::{
    collections::HashMap,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

const URL_SCHEME_SEPARATOR: &str = "://";
const NETWORK_ENDPOINT_URL_SCHEMES: [&str; 3] = ["http", "https", "unix"];
const MAX_RETRIES: u32 = 100;
const MAX_BATCH_SIZE: u32 = 100_000;
const MAX_TIMEOUT_MS: u32 = 300_000;

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
    /// Diagnostic-only structured trace payload; not a runtime wire format
    pub payload: String,
    /// Timestamp of the trace (milliseconds since epoch)
    pub timestamp_ms: u64,
    /// Trace attributes for filtering and aggregation
    pub attributes: HashMap<String, String>,
}

impl ExportDecisionTrace {
    /// Validate the trace structure.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.is_empty() {
            return Err(exporter_error(
                AndromedaErrorKind::Protocol,
                "trace id must not be empty",
            ));
        }

        if self.operation.is_empty() {
            return Err(exporter_error(
                AndromedaErrorKind::Protocol,
                "operation must not be empty",
            ));
        }

        if self.decision.is_empty() {
            return Err(exporter_error(
                AndromedaErrorKind::Protocol,
                "decision must not be empty",
            ));
        }

        if self.timestamp_ms == 0 {
            return Err(exporter_error(
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
    pub labels: HashMap<String, String>,
}

impl Metric {
    /// Validate the metric structure.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.is_empty() {
            return Err(exporter_error(
                AndromedaErrorKind::Protocol,
                "metric name must not be empty",
            ));
        }

        if self.unit.is_empty() {
            return Err(exporter_error(
                AndromedaErrorKind::Protocol,
                "metric unit must not be empty",
            ));
        }

        if self.timestamp_ms == 0 {
            return Err(exporter_error(
                AndromedaErrorKind::Protocol,
                "timestamp must not be zero",
            ));
        }

        if !self.value.is_finite() {
            return Err(exporter_error(
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
                    return Err(exporter_error(
                        AndromedaErrorKind::Protocol,
                        "delay_ms must not be zero",
                    ));
                }
                if max_retries > MAX_RETRIES {
                    return Err(exporter_error(
                        AndromedaErrorKind::Resource,
                        "max_retries must not exceed 100",
                    ));
                }
                Ok(())
            },
            Self::ExponentialBackoff {
                max_retries,
                initial_delay_ms,
            } => {
                if initial_delay_ms == 0 {
                    return Err(exporter_error(
                        AndromedaErrorKind::Protocol,
                        "initial_delay_ms must not be zero",
                    ));
                }
                if max_retries > MAX_RETRIES {
                    return Err(exporter_error(
                        AndromedaErrorKind::Resource,
                        "max_retries must not exceed 100",
                    ));
                }
                Ok(())
            },
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
                    return Err(exporter_error(
                        AndromedaErrorKind::Protocol,
                        "endpoint_url must not be empty for network backends",
                    ));
                }
                if !NETWORK_ENDPOINT_URL_SCHEMES
                    .iter()
                    .any(|scheme| starts_with_url_scheme(&self.endpoint_url, scheme))
                {
                    return Err(exporter_error(
                        AndromedaErrorKind::Protocol,
                        "endpoint_url must be a valid URL",
                    ));
                }
            },
            ExporterBackend::LocalFile => {
                if self.endpoint_url.is_empty() {
                    return Err(exporter_error(
                        AndromedaErrorKind::Protocol,
                        "endpoint_url must be a file path for LocalFile backend",
                    ));
                }
            },
            ExporterBackend::Null => {},
        }

        if self.batch_size == 0 {
            return Err(exporter_error(
                AndromedaErrorKind::Protocol,
                "batch_size must not be zero",
            ));
        }

        if self.batch_size > MAX_BATCH_SIZE {
            return Err(exporter_error(
                AndromedaErrorKind::Resource,
                "batch_size must not exceed 100000",
            ));
        }

        if self.timeout_ms == 0 {
            return Err(exporter_error(
                AndromedaErrorKind::Protocol,
                "timeout_ms must not be zero",
            ));
        }

        if self.timeout_ms > MAX_TIMEOUT_MS {
            return Err(exporter_error(
                AndromedaErrorKind::Resource,
                "timeout_ms must not exceed 300000 (5 minutes)",
            ));
        }

        self.retry_policy.validate()?;

        Ok(())
    }
}

fn starts_with_url_scheme(endpoint_url: &str, scheme: &str) -> bool {
    match endpoint_url.strip_prefix(scheme) {
        Some(remainder) => remainder.starts_with(URL_SCHEME_SEPARATOR),
        None => false,
    }
}

fn exporter_error(kind: AndromedaErrorKind, message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(kind, message)
}

/// Boundary implemented by every observability exporter backend.
pub trait ExporterTrait: Send + Sync {
    /// Export a single decision trace.
    fn export_trace(&self, trace: ExportDecisionTrace) -> AndromedaResult<()>;

    /// Export a single metric.
    fn export_metric(&self, metric: Metric) -> AndromedaResult<()>;

    /// Export a batch of traces and metrics together.
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

const EXPORTER_UNHEALTHY_MESSAGE: &str = "exporter is not healthy";

/// Mock implementation of `ExporterTrait` for testing.
#[derive(Debug, Clone)]
pub struct MockExporter {
    config: ExporterConfig,
    traces: Arc<Mutex<Vec<ExportDecisionTrace>>>,
    metrics: Arc<Mutex<Vec<Metric>>>,
    successful_count: Arc<AtomicU64>,
    failed_count: Arc<AtomicU64>,
    healthy: Arc<Mutex<bool>>,
}

impl MockExporter {
    /// Create a new mock exporter with the given configuration.
    pub fn new(config: ExporterConfig) -> AndromedaResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            traces: Arc::new(Mutex::new(Vec::new())),
            metrics: Arc::new(Mutex::new(Vec::new())),
            successful_count: Arc::new(AtomicU64::new(0)),
            failed_count: Arc::new(AtomicU64::new(0)),
            healthy: Arc::new(Mutex::new(true)),
        })
    }

    /// Get a copy of all exported traces.
    pub fn get_traces(&self) -> Vec<ExportDecisionTrace> {
        self.traces
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Get a copy of all exported metrics.
    pub fn get_metrics(&self) -> Vec<Metric> {
        self.metrics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Simulate a failure by marking the exporter unhealthy.
    pub fn simulate_failure(&self) {
        *self
            .healthy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
    }

    /// Restore health.
    pub fn restore_health(&self) {
        *self
            .healthy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
    }

    fn reject_if_unhealthy(&self, failed_count: u64) -> AndromedaResult<()> {
        if *self
            .healthy
            .lock()
            .map_err(|_| exporter_poisoned("health state"))?
        {
            return Ok(());
        }

        self.failed_count.fetch_add(failed_count, Ordering::SeqCst);
        Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            EXPORTER_UNHEALTHY_MESSAGE,
        ))
    }

    fn record_successful(&self, count: u64) {
        self.successful_count.fetch_add(count, Ordering::SeqCst);
    }
}

impl ExporterTrait for MockExporter {
    fn export_trace(&self, trace: ExportDecisionTrace) -> AndromedaResult<()> {
        trace.validate()?;

        self.reject_if_unhealthy(1)?;

        self.traces
            .lock()
            .map_err(|_| exporter_poisoned("trace buffer"))?
            .push(trace);
        self.record_successful(1);
        Ok(())
    }

    fn export_metric(&self, metric: Metric) -> AndromedaResult<()> {
        metric.validate()?;

        self.reject_if_unhealthy(1)?;

        self.metrics
            .lock()
            .map_err(|_| exporter_poisoned("metric buffer"))?
            .push(metric);
        self.record_successful(1);
        Ok(())
    }

    fn batch_export(
        &self,
        traces: Vec<ExportDecisionTrace>,
        metrics: Vec<Metric>,
    ) -> AndromedaResult<()> {
        for trace in &traces {
            trace.validate()?;
        }
        for metric in &metrics {
            metric.validate()?;
        }

        let count = (traces.len() + metrics.len()) as u64;
        self.reject_if_unhealthy(count)?;

        self.traces
            .lock()
            .map_err(|_| exporter_poisoned("trace buffer"))?
            .extend(traces);
        self.metrics
            .lock()
            .map_err(|_| exporter_poisoned("metric buffer"))?
            .extend(metrics);
        self.record_successful(count);

        Ok(())
    }

    fn config(&self) -> &ExporterConfig {
        &self.config
    }

    fn is_healthy(&self) -> bool {
        self.healthy.lock().map(|healthy| *healthy).unwrap_or(false)
    }

    fn successful_exports(&self) -> u64 {
        self.successful_count.load(Ordering::SeqCst)
    }

    fn failed_exports(&self) -> u64 {
        self.failed_count.load(Ordering::SeqCst)
    }
}

fn exporter_poisoned(resource: &str) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Internal,
        format!("mock exporter {resource} lock is poisoned"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn null_config() -> ExporterConfig {
        ExporterConfig {
            backend: ExporterBackend::Null,
            endpoint_url: "null".to_string(),
            batch_size: 100,
            timeout_ms: 5000,
            retry_policy: RetryPolicy::NoRetry,
        }
    }

    fn trace(trace_id: &str, timestamp_ms: u64) -> ExportDecisionTrace {
        ExportDecisionTrace {
            trace_id: trace_id.to_string(),
            operation: "op".to_string(),
            decision: "allowed".to_string(),
            payload: "{}".to_string(),
            timestamp_ms,
            attributes: HashMap::new(),
        }
    }

    fn metric(name: &str, value: f64, timestamp_ms: u64) -> Metric {
        Metric {
            name: name.to_string(),
            value,
            unit: "ms".to_string(),
            timestamp_ms,
            labels: HashMap::new(),
        }
    }

    #[test]
    fn mock_exporter_exports_single_trace() {
        let exporter = MockExporter::new(null_config()).unwrap();
        assert!(exporter.is_healthy());

        assert!(exporter.export_trace(trace("trace-001", 1000)).is_ok());
        assert_eq!(exporter.successful_exports(), 1);
        assert_eq!(exporter.failed_exports(), 0);
        assert_eq!(exporter.get_traces().len(), 1);
    }

    #[test]
    fn mock_exporter_exports_single_metric() {
        let exporter = MockExporter::new(null_config()).unwrap();

        assert!(
            exporter
                .export_metric(metric("test_metric", 42.0, 1000))
                .is_ok()
        );
        assert_eq!(exporter.successful_exports(), 1);
        assert_eq!(exporter.failed_exports(), 0);
        assert_eq!(exporter.get_metrics().len(), 1);
    }

    #[test]
    fn mock_exporter_exports_batches() {
        let exporter = MockExporter::new(null_config()).unwrap();

        assert!(
            exporter
                .batch_export(
                    vec![trace("trace-001", 1000), trace("trace-002", 2000)],
                    vec![metric("metric1", 10.0, 1000), metric("metric2", 20.0, 2000)],
                )
                .is_ok()
        );

        assert_eq!(exporter.successful_exports(), 4);
        assert_eq!(exporter.failed_exports(), 0);
        assert_eq!(exporter.get_traces().len(), 2);
        assert_eq!(exporter.get_metrics().len(), 2);
    }

    #[test]
    fn mock_exporter_tracks_failures() {
        let exporter = MockExporter::new(null_config()).unwrap();

        assert!(exporter.export_trace(trace("trace-001", 1000)).is_ok());
        exporter.simulate_failure();
        assert!(!exporter.is_healthy());
        assert!(exporter.export_trace(trace("trace-002", 2000)).is_err());

        assert_eq!(exporter.successful_exports(), 1);
        assert_eq!(exporter.failed_exports(), 1);

        exporter.restore_health();
        assert!(exporter.is_healthy());
        assert!(exporter.export_trace(trace("trace-003", 3000)).is_ok());

        assert_eq!(exporter.successful_exports(), 2);
        assert_eq!(exporter.failed_exports(), 1);
    }
}
