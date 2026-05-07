//! Runtime event emission abstraction.
//!
//! `EventSink` is the typed write surface (defined in [`crate::events`]). The
//! [`EventEmitter`] wraps any `EventSink` and adds:
//!
//! * monotonic, non-zero [`EventId`] allocation,
//! * pre-validation through [`EventEnvelope::new`] (no silent drops),
//! * accepted vs rejected counters so callers can prove that emission failures
//!   were observed rather than swallowed,
//! * an exhaustion guard once the `u128` event-id space is consumed.
//!
//! This sink is in-process only. It is not a durable log, not a fan-out bus,
//! and intentionally has no async surface. Durability claims must come from
//! the storage/WAL layers, never from a sink that lives in RAM.

mod in_memory_query;
mod runtime;

pub use runtime::EventEmitter;

#[cfg(test)]
mod tests;
