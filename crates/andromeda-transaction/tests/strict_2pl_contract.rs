//! Strict two-phase locking (2PL) contract tests.
//!
//! These tests validate that transaction state transitions enforce 2PL
//! discipline: lock acquisition only during the growing phase, lock release
//! during shrinking, and terminal cleanup only after durable terminal evidence.

#[path = "strict_2pl_contract/support.rs"]
mod support;

#[path = "strict_2pl_contract/acquire_rules.rs"]
mod acquire_rules;
#[path = "strict_2pl_contract/lifecycle.rs"]
mod lifecycle;
#[path = "strict_2pl_contract/release_rules.rs"]
mod release_rules;
#[path = "strict_2pl_contract/terminal_rules.rs"]
mod terminal_rules;
