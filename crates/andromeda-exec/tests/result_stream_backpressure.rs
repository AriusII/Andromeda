//! ResultStream backpressure and terminal completion contract tests.
//!
//! This integration crate keeps ResultStream assertions grouped by protocol
//! phase: metadata ordering, backpressure evidence, terminal completion, and
//! concurrent producer/consumer behavior.

#[path = "result_stream_backpressure/backpressure.rs"]
mod backpressure;
#[path = "result_stream_backpressure/completion.rs"]
mod completion;
#[path = "result_stream_backpressure/concurrency.rs"]
mod concurrency;
#[path = "result_stream_backpressure/metadata.rs"]
mod metadata;
#[path = "result_stream_backpressure/support.rs"]
mod support;
