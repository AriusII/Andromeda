//! Observability exporter trait boundary and configuration contracts.
//!
//! Exporters validate their inputs and report every backend failure through
//! `AndromedaResult`, whether data is emitted one item at a time or in batches.

mod contract;
mod mock;

pub use andromeda_observability::{
    ExportDecisionTrace, ExporterBackend, ExporterConfig, Metric, RetryPolicy,
};
pub use contract::ExporterTrait;
pub use mock::MockExporter;

#[cfg(test)]
mod tests;
