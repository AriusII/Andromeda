//! Compatibility re-exports for HA/DR logical stream mapping.
//!
//! The logical HA/DR stream allocation model now lives in `andromeda-hadr`.
//! QUIC keeps this module only as the historical import path while callers
//! migrate.

pub use andromeda_hadr::hadr_streams::*;
