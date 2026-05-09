#![forbid(unsafe_code)]

//! End-to-end B-Tree invariant tests split by contract area.

#[path = "btree_invariants_e2e/concurrency.rs"]
mod concurrency;
#[path = "btree_invariants_e2e/engine_flow.rs"]
mod engine_flow;
#[path = "btree_invariants_e2e/node_ordering.rs"]
mod node_ordering;
#[path = "btree_invariants_e2e/splits.rs"]
mod splits;
#[path = "btree_invariants_e2e/support.rs"]
mod support;
