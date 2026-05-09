#![forbid(unsafe_code)]

//! Comprehensive tests for the B+ Tree index engine.
//!
//! The focused modules below cover node construction, leaf APIs, splitting,
//! serialization, in-memory engine operations, and local invariants.

#[path = "btree_engine/engine_ops.rs"]
mod engine_ops;
#[path = "btree_engine/invariants.rs"]
mod invariants;
#[path = "btree_engine/leaf_api.rs"]
mod leaf_api;
#[path = "btree_engine/node_basics.rs"]
mod node_basics;
#[path = "btree_engine/serialization.rs"]
mod serialization;
#[path = "btree_engine/splits.rs"]
mod splits;
#[path = "btree_engine/support.rs"]
mod support;
