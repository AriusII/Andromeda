//! MVCC Garbage Collection Durability & Correctness Contract Tests
//!
//! This module validates the MVCC garbage collection engine against the
//! following key invariants:
//!
//! **PRIMARY INVARIANT:** No visible version shall ever be reclaimed.
//!
//! A version is visible if:
//! 1. Creator transaction is committed (per V0 doctrine), AND
//! 2. begin_ts <= snapshot.timestamp, AND
//! 3. end_ts == None OR end_ts > snapshot.timestamp
//!
//! A version is reclaimable only if:
//! 1. Creator is committed OR creator was rolled back, AND
//! 2. end_ts < minimum_visible_timestamp (all snapshots see it as deleted), AND
//! 3. end_ts != u64::MAX (version is closed)
//!
//! **SECONDARY INVARIANTS:**
//! - GC statistics accurately reflect scans and reclamations
//! - Active snapshots prevent GC of otherwise-eligible versions
//! - Manager hook integration triggers GC at correct thresholds
//! - Concurrent commits and GC have no races
//! - GC trace emissions correlate with start/complete boundaries

#[path = "mvcc_gc_durability_contract/correctness.rs"]
mod correctness;
#[path = "mvcc_gc_durability_contract/fixtures.rs"]
mod fixtures;
#[path = "mvcc_gc_durability_contract/scheduler_stats.rs"]
mod scheduler_stats;
#[path = "mvcc_gc_durability_contract/stress.rs"]
mod stress;
