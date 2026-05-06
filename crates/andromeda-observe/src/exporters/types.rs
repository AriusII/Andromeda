use std::{collections::HashMap, fmt};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

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
    /// Structured trace data (JSON-formatted)
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
            }
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
            }
            ExporterBackend::LocalFile => {
                if self.endpoint_url.is_empty() {
                    return Err(exporter_error(
                        AndromedaErrorKind::Protocol,
                        "endpoint_url must be a file path for LocalFile backend",
                    ));
                }
            }
            ExporterBackend::Null => {}
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
