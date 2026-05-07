//! Comprehensive contract tests for SRPL execution adapters.
//!
//! This test suite validates all 5 adapters across 50+ test cases covering:
//! - Type cardinality preservation
//! - Error determinism and reproducibility
//! - Predicate binding via scoped type environment
//! - Stream backpressure (no unbounded buffering)
//! - Transaction atomicity (all-or-nothing per invocation)

#[path = "srpl_adapter_contract/assert_adapter.rs"]
mod assert_adapter;
#[path = "srpl_adapter_contract/read_adapter.rs"]
mod read_adapter;
#[path = "srpl_adapter_contract/support.rs"]
mod support;
#[path = "srpl_adapter_contract/transaction_backpressure_environment.rs"]
mod transaction_backpressure_environment;
#[path = "srpl_adapter_contract/update_emit_failure_adapters.rs"]
mod update_emit_failure_adapters;
#[path = "srpl_adapter_contract/workflow_contracts.rs"]
mod workflow_contracts;
