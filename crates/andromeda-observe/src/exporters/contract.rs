use andromeda_error::AndromedaResult;

use super::{ExportDecisionTrace, ExporterConfig, Metric};

/// Boundary implemented by every observability exporter backend.
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
