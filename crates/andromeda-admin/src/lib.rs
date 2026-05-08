#![forbid(unsafe_code)]
//! Administration facade boundary for Andromeda.
//!
//! This crate intentionally owns no runtime state, durable storage, HA/DR
//! behavior, or application-facing procedure execution.

/// Marker for the administration facade crate boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdminFacadeBoundary;
