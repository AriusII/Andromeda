#![forbid(unsafe_code)]

//! B-Tree KeyV1 format validation gate for DEC-038.
//!
//! This module implements format validation gates required by DEC-032 and
//! DEC-038:
//! - validates persisted B-Tree indexes have recognized key-format identity;
//! - rejects mutations with explicit deferral messages;
//! - allows read-only operations;
//! - enforces fail-fast behavior on format mismatches;
//! - provides deterministic validation results across repeated calls.
//!
//! Page-backed B-Tree mutations are not promoted yet. This gate preserves format
//! safety by allowing read-only access and rejecting mutation attempts before any
//! page or WAL state can be changed.

mod identity;
mod operation;
mod validation;
mod validator;

pub use identity::BTreeKeyFormatIdentity;
pub use operation::BTreeOperationType;
pub use validator::KeyV1FormatValidator;

#[cfg(test)]
mod tests;
