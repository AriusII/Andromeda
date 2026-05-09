#![forbid(unsafe_code)]

//! WAL durability fence contract suite.
//!
//! The focused modules cover:
//!
//! - page flush fencing against durable WAL
//! - manifest checkpoint atomicity
//! - recovery floor and LSN monotonicity
//! - crash/checkpoint workflows that compose the gates

#[path = "wal_durability_fence_contract/manifest_switch.rs"]
mod manifest_switch;
#[path = "wal_durability_fence_contract/page_flush.rs"]
mod page_flush;
#[path = "wal_durability_fence_contract/recovery_floor.rs"]
mod recovery_floor;
#[path = "wal_durability_fence_contract/support.rs"]
mod support;
#[path = "wal_durability_fence_contract/workflows.rs"]
mod workflows;
