#![forbid(unsafe_code)]

//! B-Tree concurrency contract tests split by policy area.

#[path = "btree_concurrency_contract/acquisition_order.rs"]
mod acquisition_order;
#[path = "btree_concurrency_contract/latch_modes.rs"]
mod latch_modes;
#[path = "btree_concurrency_contract/policy.rs"]
mod policy;
#[path = "btree_concurrency_contract/restart.rs"]
mod restart;
#[path = "btree_concurrency_contract/support.rs"]
mod support;
