#![forbid(unsafe_code)]

//! B-Tree durable format promotion gate tests - DEC-038 validation.
//!
//! The modules validate durable mutation deferral, KeyV1 compatibility,
//! read-only access, unknown-format fail-fast behavior, deterministic errors,
//! and recovery-oriented doctrine checks.

#[path = "btree_format_deferral_gate/compatibility.rs"]
mod compatibility;
#[path = "btree_format_deferral_gate/determinism.rs"]
mod determinism;
#[path = "btree_format_deferral_gate/doctrine.rs"]
mod doctrine;
#[path = "btree_format_deferral_gate/format_identity.rs"]
mod format_identity;
#[path = "btree_format_deferral_gate/mutation_gate.rs"]
mod mutation_gate;
#[path = "btree_format_deferral_gate/operation_names.rs"]
mod operation_names;
#[path = "btree_format_deferral_gate/read_and_unknown_formats.rs"]
mod read_and_unknown_formats;
#[path = "btree_format_deferral_gate/support.rs"]
mod support;
