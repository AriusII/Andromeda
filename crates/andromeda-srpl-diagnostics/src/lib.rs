#![forbid(unsafe_code)]

//! SRPL source-location and diagnostic primitives.
//!
//! This crate owns parser-independent SRPL diagnostics. It does not parse,
//! bind, lower, execute, or consult catalog storage.

mod diagnostics;
pub mod source_location;

pub use diagnostics::{DiagnosticPhase, ForbiddenConstruct, ForbiddenConstructHit, SrplDiagnostic};
pub use source_location::{SourceSpan, SrplSource};
