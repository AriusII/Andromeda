#![forbid(unsafe_code)]
//! Runtime-free boundary for future client SDK generation.
//!
//! SDK generation may consume explicit, versioned contract inputs. It must not
//! own protocol transport, server execution, catalog storage, WAL, or recovery
//! behavior.

/// Marker for the client SDK generation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientSdkGenerationBoundary;

/// Backward-compatible marker alias for the scaffold crate.
pub type AndromedaClientSdkGen = ClientSdkGenerationBoundary;
