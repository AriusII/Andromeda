//! Observability exporter trait boundary and configuration contracts.
//!
//! Exporters validate their inputs and report every backend failure through
//! `AndromedaResult`, whether data is emitted one item at a time or in batches.

mod contract;
mod mock;
mod types;

pub use contract::ExporterTrait;
pub use mock::MockExporter;
pub use types::{ExportDecisionTrace, ExporterBackend, ExporterConfig, Metric, RetryPolicy};

#[cfg(test)]
mod tests;
