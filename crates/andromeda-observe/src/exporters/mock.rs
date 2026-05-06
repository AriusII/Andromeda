use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::{ExportDecisionTrace, ExporterConfig, ExporterTrait, Metric};

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

    fn reject_if_unhealthy(&self, failed_count: u64) -> AndromedaResult<()> {
        if *self.healthy.lock().unwrap() {
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

        self.traces.lock().unwrap().push(trace);
        self.record_successful(1);
        Ok(())
    }

    fn export_metric(&self, metric: Metric) -> AndromedaResult<()> {
        metric.validate()?;

        self.reject_if_unhealthy(1)?;

        self.metrics.lock().unwrap().push(metric);
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

        self.traces.lock().unwrap().extend(traces);
        self.metrics.lock().unwrap().extend(metrics);
        self.record_successful(count);

        Ok(())
    }

    fn config(&self) -> &ExporterConfig {
        &self.config
    }

    fn is_healthy(&self) -> bool {
        *self.healthy.lock().unwrap()
    }

    fn successful_exports(&self) -> u64 {
        self.successful_count.load(Ordering::SeqCst)
    }

    fn failed_exports(&self) -> u64 {
        self.failed_count.load(Ordering::SeqCst)
    }
}
